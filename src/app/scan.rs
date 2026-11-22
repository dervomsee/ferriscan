//! Scan use case shared by the CLI and future GUI front ends.

use crate::core::{Result, ScanSettings, ScanSummary, Sheet};
use crate::ports::{ScanOutput, ScannerPort, WrittenPage};

/// Run one scan: drive `scanner`, persist each sheet through `output`, and report the summary.
///
/// `on_sheet` receives the 1-based sheet index, the sheet, and the pages just written, so a front
/// end can log progress or update a UI without touching sockets or the filesystem. It runs on the
/// scanning thread and must not block for long.
pub fn run(
    scanner: &mut dyn ScannerPort,
    output: &mut dyn ScanOutput,
    settings: &ScanSettings,
    on_sheet: &mut dyn FnMut(u32, &Sheet, &[WrittenPage]),
) -> Result<ScanSummary> {
    scanner.connect()?;
    let mut index = 0_u32;
    let summary = scanner.scan(settings, &mut |sheet| {
        index += 1;
        let written = output.write(index, &sheet)?;
        on_sheet(index, &sheet, &written);
        Ok(())
    })?;
    scanner.disconnect()?;
    Ok(summary)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::core::{Error, PageSide};

    struct FakeScanner {
        sheets: u32,
        connected: bool,
        disconnected: bool,
    }

    impl ScannerPort for FakeScanner {
        fn connect(&mut self) -> Result<()> {
            self.connected = true;
            Ok(())
        }

        fn disconnect(&mut self) -> Result<()> {
            self.disconnected = true;
            Ok(())
        }

        fn scan(
            &mut self,
            _settings: &ScanSettings,
            on_sheet: &mut dyn FnMut(Sheet) -> Result<()>,
        ) -> Result<ScanSummary> {
            let mut summary = ScanSummary::default();
            for index in 0..self.sheets {
                on_sheet(Sheet {
                    front: vec![u8::try_from(index).unwrap()],
                    back: None,
                    info: None,
                })?;
                summary.sheets += 1;
                summary.images += 1;
            }
            Ok(summary)
        }
    }

    struct FakeOutput {
        written: u32,
        fail_on: Option<u32>,
    }

    impl ScanOutput for FakeOutput {
        fn write(&mut self, index: u32, _sheet: &Sheet) -> Result<Vec<WrittenPage>> {
            if self.fail_on == Some(index) {
                return Err(Error::Input("output failed".into()));
            }
            self.written += 1;
            Ok(vec![WrittenPage {
                side: PageSide::Front,
                path: PathBuf::from(format!("page{index:03}_front.jpg")),
            }])
        }
    }

    #[test]
    fn delivers_every_sheet_and_reports_the_summary() {
        let mut scanner = FakeScanner {
            sheets: 3,
            connected: false,
            disconnected: false,
        };
        let mut output = FakeOutput {
            written: 0,
            fail_on: None,
        };
        let mut reported = Vec::new();
        let summary = run(
            &mut scanner,
            &mut output,
            &ScanSettings::default(),
            &mut |index, _sheet, written| {
                reported.push((index, written.len()));
            },
        )
        .unwrap();

        assert_eq!(summary.sheets, 3);
        assert_eq!(summary.images, 3);
        assert_eq!(output.written, 3);
        assert_eq!(reported, [(1, 1), (2, 1), (3, 1)]);
        assert!(scanner.connected);
        assert!(scanner.disconnected);
    }

    #[test]
    fn stops_at_the_first_output_failure() {
        let mut scanner = FakeScanner {
            sheets: 3,
            connected: false,
            disconnected: false,
        };
        let mut output = FakeOutput {
            written: 0,
            fail_on: Some(2),
        };
        let mut reported = 0;
        let error = run(
            &mut scanner,
            &mut output,
            &ScanSettings::default(),
            &mut |_, _, _| reported += 1,
        )
        .unwrap_err();

        assert!(matches!(error, Error::Input(message) if message == "output failed"));
        assert_eq!(output.written, 1);
        assert_eq!(reported, 1);
        // The scan stopped on the failing sheet, so the session is not cleanly disconnected.
        assert!(!scanner.disconnected);
    }
}
