//! Persistent iX500 host identity and scanner password.
//!
//! The iX500 validates the scanner Wi-Fi password on every registration (0x11), so a client that wants to
//! scan without retyping the password must remember it. The identity file also stores the paired host ID,
//! which the scanner only writes over USB and which this client must reuse.
//!
//! The file contains the scanner Wi-Fi password in plain text and is therefore created with mode 0600.

use std::fs;
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};

use super::control::{DEFAULT_NOTIFICATION_PORT, HostIdentity};
use crate::adapters::scansnap_wifi::identity::{format_hex, format_mac, parse_mac};
use crate::core::{Error, Result};

pub const DEFAULT_FILE_NAME: &str = "ix500.conf";

/// Host identity and optional scanner password as stored on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredIdentity {
    pub host_id: [u8; 8],
    pub mac: [u8; 6],
    pub notification_port: u16,
    /// `None` means no password is recorded; `Some("")` records that the scanner has no password.
    pub password: Option<String>,
}

impl StoredIdentity {
    #[must_use]
    pub fn host_identity(&self) -> HostIdentity {
        HostIdentity {
            host_id: self.host_id,
            mac: self.mac,
            notification_port: self.notification_port,
        }
    }

    /// Load the identity, returning `Ok(None)` when the file does not exist.
    pub fn load(path: &Path) -> Result<Option<Self>> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        parse(&text)
            .map(Some)
            .map_err(|error| Error::Input(format!("{}: {error}", path.display())))
    }

    /// Write the identity atomically with owner-only permissions.
    pub fn save(&self, path: &Path) -> Result<()> {
        let mut lines = vec![
            "# Ferriscan iX500 identity. This file contains the scanner Wi-Fi password in plain text."
                .to_owned(),
            format!("host_id = {}", format_hex(&self.host_id)),
            format!("mac = {}", format_mac(self.mac)),
            format!("notification_port = {}", self.notification_port),
        ];
        if let Some(password) = &self.password {
            lines.push(format!("password = {password}"));
        }
        let text = format!("{}\n", lines.join("\n"));

        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty());
        if let Some(parent) = parent {
            fs::create_dir_all(parent)?;
            restrict_directory(parent);
        }
        let temporary = path.with_extension("conf.tmp");
        write_private(&temporary, &text)?;
        fs::rename(&temporary, path)?;
        Ok(())
    }
}

fn parse(text: &str) -> Result<StoredIdentity> {
    let mut host_id = None;
    let mut mac = None;
    let mut notification_port = DEFAULT_NOTIFICATION_PORT;
    let mut password = None;

    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            return Err(Error::Input(format!(
                "line {}: expected 'key = value'",
                index + 1
            )));
        };
        match key.trim() {
            "host_id" => host_id = Some(parse_host_id(value.trim())?),
            "mac" => mac = Some(parse_mac(value.trim())?),
            "notification_port" => {
                notification_port = value.trim().parse().map_err(|_| {
                    Error::Input(format!(
                        "line {}: invalid notification port '{}'",
                        index + 1,
                        value.trim()
                    ))
                })?;
            }
            "password" => password = Some(value.trim().to_owned()),
            other => {
                return Err(Error::Input(format!(
                    "line {}: unknown key '{other}'",
                    index + 1
                )));
            }
        }
    }

    Ok(StoredIdentity {
        host_id: host_id.ok_or_else(|| Error::Input("missing 'host_id'".into()))?,
        mac: mac.ok_or_else(|| Error::Input("missing 'mac'".into()))?,
        notification_port,
        password,
    })
}

fn parse_host_id(value: &str) -> Result<[u8; 8]> {
    let mut host_id = [0_u8; 8];
    if value.len() != host_id.len() * 2 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(Error::Input(format!(
            "host ID '{value}' must be 16 hexadecimal characters"
        )));
    }
    for (index, byte) in host_id.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16)
            .map_err(|_| Error::Input(format!("invalid host ID '{value}'")))?;
    }
    if host_id == [0; 8] {
        return Err(Error::Input("host ID must not be all zeros".into()));
    }
    Ok(host_id)
}

/// Default identity location, following the XDG base directory specification.
pub fn default_path() -> Result<PathBuf> {
    if let Some(base) = std::env::var_os("XDG_CONFIG_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(base)
            .join("ferriscan")
            .join(DEFAULT_FILE_NAME));
    }
    let home = std::env::var_os("HOME")
        .ok_or_else(|| Error::Input("neither XDG_CONFIG_HOME nor HOME is set".into()))?;
    Ok(PathBuf::from(home)
        .join(".config")
        .join("ferriscan")
        .join(DEFAULT_FILE_NAME))
}

#[cfg(not(unix))]
fn write_private(path: &Path, contents: &str) -> Result<()> {
    fs::write(path, contents).map_err(Into::into)
}

#[cfg(unix)]
fn write_private(path: &Path, contents: &str) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt as _;

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    IoWrite::write_all(&mut file, contents.as_bytes())?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_directory(_path: &Path) {}

#[cfg(unix)]
fn restrict_directory(path: &Path) {
    use std::os::unix::fs::PermissionsExt as _;

    // Best effort: the directory only holds this credential file.
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOST_ID: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];
    const MAC: [u8; 6] = [0x34, 0xc9, 0x3d, 0x9a, 0x04, 0xa2];

    fn identity() -> StoredIdentity {
        StoredIdentity {
            host_id: HOST_ID,
            mac: MAC,
            notification_port: DEFAULT_NOTIFICATION_PORT,
            password: Some("0000".into()),
        }
    }

    #[test]
    fn parses_comments_defaults_and_optional_password() {
        let parsed = parse(
            "# comment\n\nhost_id = 0102030405060708\nmac = 34:c9:3d:9a:04:a2\npassword = 0000\n",
        )
        .unwrap();
        assert_eq!(parsed, identity());

        let parsed = parse("host_id=0102030405060708\nmac=34-c9-3d-9a-04-a2\n").unwrap();
        assert_eq!(parsed.password, None);
        assert_eq!(parsed.notification_port, DEFAULT_NOTIFICATION_PORT);
    }

    #[test]
    fn rejects_malformed_files_instead_of_guessing() {
        assert!(
            parse("mac = 34:c9:3d:9a:04:a2\n")
                .unwrap_err()
                .to_string()
                .contains("host_id")
        );
        assert!(
            parse("host_id = 0102030405060708\n")
                .unwrap_err()
                .to_string()
                .contains("mac")
        );
        assert!(parse("host_id = 0000000000000000\nmac = 34:c9:3d:9a:04:a2\n").is_err());
        assert!(
            parse("host_id = 0102030405060708\nmac = 34:c9:3d:9a:04:a2\nsurprise = 1\n").is_err()
        );
        assert!(parse("host_id 0102030405060708\n").is_err());
        assert!(
            parse("host_id = 0102030405060708\nmac = 34:c9:3d:9a:04:a2\nnotification_port = x\n")
                .is_err()
        );
    }

    #[test]
    fn exposes_the_control_host_identity() {
        let control = identity().host_identity();
        assert_eq!(control.host_id, HOST_ID);
        assert_eq!(control.mac, MAC);
        assert_eq!(control.notification_port, DEFAULT_NOTIFICATION_PORT);
    }

    #[test]
    fn round_trips_through_a_private_file() {
        let directory = std::env::temp_dir().join(format!(
            "ferriscan-identity-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let path = directory.join(DEFAULT_FILE_NAME);
        let _ = fs::remove_dir_all(&directory);

        assert!(StoredIdentity::load(&path).unwrap().is_none());
        identity().save(&path).unwrap();
        assert_eq!(StoredIdentity::load(&path).unwrap(), Some(identity()));

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(
                mode, 0o600,
                "identity file must not be group or world readable"
            );
        }

        fs::remove_dir_all(&directory).unwrap();
    }
}
