//! Filesystem scan output with atomic writes and overwrite protection.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use crate::core::{Error, PageSide, Result, Sheet};
use crate::ports::{ScanOutput, WrittenPage};

/// Writes `pageNNN_front.jpg` and, when present, `pageNNN_back.jpg` into one directory.
pub struct FileScanOutput {
    directory: PathBuf,
    overwrite: bool,
}

impl FileScanOutput {
    /// Create the output directory if needed. With `overwrite`, existing page files are replaced;
    /// otherwise a write fails before touching the filesystem.
    pub fn new(directory: PathBuf, overwrite: bool) -> Result<Self> {
        fs::create_dir_all(&directory)?;
        Ok(Self {
            directory,
            overwrite,
        })
    }

    fn page_path(&self, index: u32, side: PageSide) -> PathBuf {
        let name = match side {
            PageSide::Front => format!("page{index:03}_front.jpg"),
            PageSide::Back => format!("page{index:03}_back.jpg"),
        };
        self.directory.join(name)
    }
}

/// Write through a temporary file so a failed write never leaves a truncated page behind.
fn write_image(path: &Path, data: &[u8]) -> Result<()> {
    let mut temporary = path.to_path_buf();
    temporary.set_extension("jpg.tmp");
    let mut file = fs::File::create(&temporary)?;
    if let Err(error) = file.write_all(data).and_then(|()| file.sync_all()) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    drop(file);
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        return Err(error.into());
    }
    Ok(())
}

impl ScanOutput for FileScanOutput {
    fn write(&mut self, index: u32, sheet: &Sheet) -> Result<Vec<WrittenPage>> {
        let mut targets = vec![(
            PageSide::Front,
            self.page_path(index, PageSide::Front),
            sheet.front.as_slice(),
        )];
        if let Some(back) = &sheet.back {
            targets.push((
                PageSide::Back,
                self.page_path(index, PageSide::Back),
                back.as_slice(),
            ));
        }

        // Refuse the whole sheet before writing either side, so a rejected back cannot leave a front.
        if !self.overwrite {
            for (_, path, _) in &targets {
                if path.exists() {
                    return Err(Error::OutputExists(path.display().to_string()));
                }
            }
        }

        let mut written = Vec::with_capacity(targets.len());
        for (side, path, data) in targets {
            write_image(&path, data)?;
            written.push(WrittenPage { side, path });
        }
        Ok(written)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_directory(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "ferriscan-output-{label}-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ))
    }

    fn sheet(front: &[u8], back: Option<&[u8]>) -> Sheet {
        Sheet {
            front: front.to_vec(),
            back: back.map(<[u8]>::to_vec),
            info: None,
        }
    }

    #[test]
    fn writes_named_front_and_back_files() {
        let directory = temporary_directory("naming");
        let _ = fs::remove_dir_all(&directory);
        let mut output = FileScanOutput::new(directory.clone(), false).unwrap();

        let written = output.write(1, &sheet(b"front", Some(b"back"))).unwrap();
        assert_eq!(
            written.iter().map(|page| page.side).collect::<Vec<_>>(),
            [PageSide::Front, PageSide::Back]
        );
        assert_eq!(written[0].path, directory.join("page001_front.jpg"));
        assert_eq!(written[1].path, directory.join("page001_back.jpg"));
        assert_eq!(fs::read(&written[0].path).unwrap(), b"front");
        assert_eq!(fs::read(&written[1].path).unwrap(), b"back");

        // No temporary files survive a successful write.
        let leftovers = fs::read_dir(&directory)
            .unwrap()
            .filter_map(std::result::Result::ok)
            .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "tmp"))
            .count();
        assert_eq!(leftovers, 0);

        fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn refuses_to_overwrite_without_the_flag() {
        let directory = temporary_directory("overwrite");
        let _ = fs::remove_dir_all(&directory);
        let mut output = FileScanOutput::new(directory.clone(), false).unwrap();
        output.write(1, &sheet(b"original", None)).unwrap();

        let error = output.write(1, &sheet(b"replacement", None)).unwrap_err();
        assert!(matches!(error, Error::OutputExists(path) if path.ends_with("page001_front.jpg")));
        // The earlier page is untouched.
        assert_eq!(
            fs::read(directory.join("page001_front.jpg")).unwrap(),
            b"original"
        );

        fs::remove_dir_all(&directory).unwrap();
    }

    #[test]
    fn overwrites_when_asked() {
        let directory = temporary_directory("force");
        let _ = fs::remove_dir_all(&directory);
        let mut output = FileScanOutput::new(directory.clone(), true).unwrap();
        output.write(1, &sheet(b"original", None)).unwrap();
        output.write(1, &sheet(b"replacement", None)).unwrap();
        assert_eq!(
            fs::read(directory.join("page001_front.jpg")).unwrap(),
            b"replacement"
        );

        fs::remove_dir_all(&directory).unwrap();
    }
}
