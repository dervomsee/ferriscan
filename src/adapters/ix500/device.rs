//! High-level iX500 device implementing the transport-neutral scanner port.

use std::net::Ipv4Addr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::control::{Control, DEFAULT_NOTIFICATION_PORT, HostIdentity, default_password};
use super::discovery::{ScannerInfo, discover_expected};
use super::identity::{self, StoredIdentity};
use super::scanner::{Scanner, release};
use super::scsi::ScsiChannel;
use crate::adapters::scansnap_wifi::discovery::local_ip_for;
use crate::core::{Error, Result, ScanSettings, ScanSummary, Sheet};
use crate::ports::ScannerPort;

pub const DISCOVERY_ATTEMPTS: u8 = 3;
pub const REGISTER_TIMEOUT: Duration = Duration::from_secs(10);
pub const SCAN_TIMEOUT: Duration = Duration::from_secs(60);

pub struct Ix500Scanner {
    ip: Ipv4Addr,
    mac: [u8; 6],
    password: Option<String>,
    identity_path: PathBuf,
    save_identity: bool,
    discovery_timeout: Duration,
    control: Option<Control>,
    info: Option<ScannerInfo>,
    inquiry: Option<(String, String, String)>,
    registered: bool,
    identity_saved: bool,
    identity_save_error: Option<String>,
}

impl Ix500Scanner {
    #[must_use]
    pub fn new(ip: Ipv4Addr, mac: [u8; 6]) -> Self {
        let identity_path =
            identity::default_path().unwrap_or_else(|_| PathBuf::from(identity::DEFAULT_FILE_NAME));
        Self {
            ip,
            mac,
            password: None,
            identity_path,
            save_identity: true,
            discovery_timeout: Duration::from_secs(1),
            control: None,
            info: None,
            inquiry: None,
            registered: false,
            identity_saved: false,
            identity_save_error: None,
        }
    }

    /// Set an explicit password. An empty string means that no password is sent.
    #[must_use]
    pub fn with_password(mut self, password: String) -> Self {
        self.password = Some(password);
        self
    }

    /// Use `path` for the persisted host identity and scanner password; `save` controls writing it.
    #[must_use]
    pub fn with_identity(mut self, path: PathBuf, save: bool) -> Self {
        self.identity_path = path;
        self.save_identity = save;
        self
    }

    #[must_use]
    pub fn with_discovery_timeout(mut self, timeout: Duration) -> Self {
        self.discovery_timeout = timeout;
        self
    }

    #[must_use]
    pub fn info(&self) -> Option<&ScannerInfo> {
        self.info.as_ref()
    }

    #[must_use]
    pub fn inquiry(&self) -> Option<&(String, String, String)> {
        self.inquiry.as_ref()
    }

    #[must_use]
    pub fn identity_path(&self) -> &Path {
        &self.identity_path
    }

    #[must_use]
    pub fn identity_saved(&self) -> bool {
        self.identity_saved
    }

    #[must_use]
    pub fn identity_save_error(&self) -> Option<&str> {
        self.identity_save_error.as_deref()
    }

    /// Device name over the control channel. Registers this host first, which the iX500 requires.
    pub fn device_name(&mut self) -> Result<String> {
        self.connect()?;
        self.control_ref()?.device_name()
    }

    /// Wi-Fi status over the control channel. Registers this host first.
    pub fn wifi_status(&mut self) -> Result<u32> {
        self.connect()?;
        self.control_ref()?.wifi_status()
    }

    fn control_ref(&self) -> Result<&Control> {
        self.control
            .as_ref()
            .ok_or_else(|| Error::Protocol("iX500 is not connected".into()))
    }

    fn scan_port(&self) -> Result<u16> {
        self.info
            .as_ref()
            .map(|info| info.scan_port)
            .ok_or_else(|| Error::Protocol("iX500 scan port is unknown before discovery".into()))
    }

    fn persist_identity(&mut self, host_id: [u8; 8], password: Option<&str>) {
        if !self.save_identity {
            return;
        }
        let stored = StoredIdentity {
            host_id,
            mac: self.mac,
            notification_port: DEFAULT_NOTIFICATION_PORT,
            password: password.map(str::to_owned),
        };
        match stored.save(&self.identity_path) {
            Ok(()) => self.identity_saved = true,
            Err(error) => self.identity_save_error = Some(error.to_string()),
        }
    }
}

/// Reuse a stored host ID; otherwise adopt the host ID the scanner reports as paired.
pub(crate) fn resolve_host_id(stored: Option<[u8; 8]>, paired: [u8; 8]) -> Result<[u8; 8]> {
    if let Some(stored) = stored {
        return Ok(stored);
    }
    if paired == [0; 8] {
        return Err(Error::Input(
            "the scanner reports no paired host ID; pair it once with ScanSnap Home over USB and reuse that host ID"
                .into(),
        ));
    }
    Ok(paired)
}

/// Explicit password, then stored password, then the factory default from the serial number.
pub(crate) fn resolve_password(
    explicit: Option<&str>,
    stored: Option<&str>,
    serial: &str,
) -> Result<Option<String>> {
    match explicit.or(stored) {
        Some("") => Ok(None),
        Some(value) => Ok(Some(value.to_owned())),
        None => Ok(Some(default_password(serial)?.to_owned())),
    }
}

impl ScannerPort for Ix500Scanner {
    fn connect(&mut self) -> Result<()> {
        if self.registered {
            return Ok(());
        }
        let info = discover_expected(
            self.mac,
            self.ip,
            DISCOVERY_ATTEMPTS,
            self.discovery_timeout,
        )?
        .ok_or_else(|| {
            Error::Protocol(format!(
                "no iX500 discovery reply from {}; check that it is powered on with its lid open, reachable, \
                 and not holding a session for another host. The scanner stays silent for a while after a session ends",
                self.ip
            ))
        })?;

        let stored = StoredIdentity::load(&self.identity_path)?;
        let host_id = resolve_host_id(
            stored.as_ref().map(|stored| stored.host_id),
            info.paired_host_id,
        )?;
        let password = resolve_password(
            self.password.as_deref(),
            stored
                .as_ref()
                .and_then(|stored| stored.password.as_deref()),
            &info.serial,
        )?;

        let identity = HostIdentity::new(host_id, self.mac);
        let control = Control::new(self.ip, identity).with_port(info.control_port);
        control.register(
            local_ip_for(self.ip)?,
            info.interface_version,
            password.as_deref(),
        )?;

        // Registration binds the scan channel to this interface MAC. Verify the channel before reporting
        // a connected device; the scanner accepts only one scan-channel client at a time.
        let mut channel =
            ScsiChannel::connect(self.ip, self.mac, info.scan_port, REGISTER_TIMEOUT)?;
        self.inquiry = Some(channel.inquiry()?);
        channel.hardware_status()?;

        self.persist_identity(host_id, password.as_deref());
        self.control = Some(control);
        self.info = Some(info);
        self.registered = true;
        Ok(())
    }

    fn disconnect(&mut self) -> Result<()> {
        // Control registration has no persistent socket in this implementation. Scan channels are closed
        // after every operation, and D6 releases each completed scan session separately.
        self.registered = false;
        self.control = None;
        Ok(())
    }

    fn scan(
        &mut self,
        settings: &ScanSettings,
        on_sheet: &mut dyn FnMut(Sheet) -> Result<()>,
    ) -> Result<ScanSummary> {
        self.connect()?;
        let port = self.scan_port()?;
        let mut channel = ScsiChannel::connect(self.ip, self.mac, port, SCAN_TIMEOUT)?;
        let scan_result = Scanner::new(&mut channel).scan(settings, on_sheet);
        drop(channel);

        // Always try D6 after opening the scan channel, but preserve the original scan error.
        let release_result = release(self.ip, self.mac, port);
        match scan_result {
            Err(error) => Err(error),
            Ok(summary) => release_result.map(|()| summary),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_host_id_wins_over_the_reported_pairing() {
        let stored = [9_u8; 8];
        assert_eq!(resolve_host_id(Some(stored), [1; 8]).unwrap(), stored);
        assert_eq!(resolve_host_id(None, [1; 8]).unwrap(), [1; 8]);
    }

    #[test]
    fn an_unpaired_scanner_is_reported_instead_of_guessed() {
        let error = resolve_host_id(None, [0; 8]).unwrap_err();
        assert!(error.to_string().contains("no paired host ID"));
    }

    #[test]
    fn password_precedence_is_explicit_then_stored_then_serial_default() {
        assert_eq!(
            resolve_password(Some("1111"), Some("2222"), "A0XX012345").unwrap(),
            Some("1111".into())
        );
        assert_eq!(
            resolve_password(None, Some("2222"), "A0XX012345").unwrap(),
            Some("2222".into())
        );
        assert_eq!(
            resolve_password(None, None, "A0XX012345").unwrap(),
            Some("2345".into())
        );
        // An explicit empty value means the scanner has no password; it must not fall back to the default.
        assert_eq!(
            resolve_password(Some(""), Some("2222"), "A0XX012345").unwrap(),
            None
        );
        assert_eq!(
            resolve_password(None, Some(""), "A0XX012345").unwrap(),
            None
        );
        assert!(resolve_password(None, None, "not-a-serial").is_err());
    }
}
