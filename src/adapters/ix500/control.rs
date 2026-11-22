//! iX500 control channel: registration, device name, and Wi-Fi status.

use std::fmt::Write as _;
use std::net::{Ipv4Addr, SocketAddrV4, TcpStream};
use std::time::Duration;

use time::OffsetDateTime;

use crate::adapters::scansnap_wifi::wire::{self, VENS};
use crate::core::{Error, Result};

pub const CONTROL_PORT: u16 = 53219;
pub const REGISTER_BODY_LEN: usize = 96;
pub const SCANNER_TIMEOUT: u8 = 0x1e;
pub const DEFAULT_NOTIFICATION_PORT: u16 = 55265;
const PASSWORD_KEY: &[u8; 16] = b"pFusCANsNapFiPfu";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HostIdentity {
    pub host_id: [u8; 8],
    pub mac: [u8; 6],
    pub notification_port: u16,
}

impl HostIdentity {
    #[must_use]
    pub fn new(host_id: [u8; 8], mac: [u8; 6]) -> Self {
        Self {
            host_id,
            mac,
            notification_port: DEFAULT_NOTIFICATION_PORT,
        }
    }
}

/// Encode the scanner password as three decimal digits per character.
pub fn encode_password(password: &str) -> Result<String> {
    if password.is_empty() || password.len() > PASSWORD_KEY.len() || !password.is_ascii() {
        return Err(Error::Input(format!(
            "password must contain 1 to {} ASCII characters",
            PASSWORD_KEY.len()
        )));
    }
    let mut encoded = String::with_capacity(password.len() * 3);
    for (character, key) in password.bytes().zip(PASSWORD_KEY) {
        let _ = write!(
            encoded,
            "{:03}",
            u16::from(character) + u16::from(*key) + 11
        );
    }
    Ok(encoded)
}

/// The factory password is the final four ASCII digits of the discovery serial number.
pub fn default_password(serial: &str) -> Result<&str> {
    if serial.len() < 4 || !serial.is_ascii() {
        return Err(Error::Input(
            "iX500 serial number must end in four ASCII digits".into(),
        ));
    }
    let password = &serial[serial.len() - 4..];
    if !password.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(Error::Input(
            "iX500 serial number must end in four ASCII digits".into(),
        ));
    }
    Ok(password)
}

pub fn register_body(
    identity: &HostIdentity,
    host_ip: Ipv4Addr,
    interface_version: u16,
    password: Option<&str>,
    now: OffsetDateTime,
) -> Result<[u8; REGISTER_BODY_LEN]> {
    let mut body = [0_u8; REGISTER_BODY_LEN];
    body[..2].copy_from_slice(&interface_version.to_be_bytes());
    body[2] = SCANNER_TIMEOUT;
    let has_password = password.is_some_and(|value| !value.is_empty());
    if has_password {
        body[8..12].copy_from_slice(&1_u32.to_be_bytes());
    }
    body[12..16].copy_from_slice(&host_ip.octets());
    body[16..20].copy_from_slice(&u32::from(identity.notification_port).to_be_bytes());
    if let Some(password) = password.filter(|_| has_password) {
        let encoded = encode_password(password)?;
        body[20..20 + encoded.len()].copy_from_slice(encoded.as_bytes());
    }

    let year = u16::try_from(now.year())
        .map_err(|_| Error::Input("registration year does not fit in u16".into()))?;
    body[68..70].copy_from_slice(&year.to_be_bytes());
    body[70] = u8::from(now.month());
    body[71] = now.day();
    body[72] = now.hour();
    body[73] = now.minute();
    body[74] = now.second();
    body[76..84].copy_from_slice(&identity.host_id);
    if interface_version > 3 {
        let bias = -now.offset().whole_seconds();
        body[84..88].copy_from_slice(&bias.to_be_bytes());
    }
    Ok(body)
}

#[derive(Debug, Clone)]
pub struct Control {
    ip: Ipv4Addr,
    port: u16,
    identity: HostIdentity,
    timeout: Duration,
}

impl Control {
    #[must_use]
    pub fn new(ip: Ipv4Addr, identity: HostIdentity) -> Self {
        Self {
            ip,
            port: CONTROL_PORT,
            identity,
            timeout: Duration::from_secs(10),
        }
    }

    #[must_use]
    pub fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }

    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    fn call(&self, command: u32, body: &[u8]) -> Result<Vec<u8>> {
        let mut stream = TcpStream::connect_timeout(
            &SocketAddrV4::new(self.ip, self.port).into(),
            self.timeout,
        )?;
        stream.set_read_timeout(Some(self.timeout))?;
        stream.set_write_timeout(Some(self.timeout))?;
        let greeting = wire::read_frame(&mut stream, VENS)?;
        if greeting.len() != wire::SERVER_HEADER_LEN {
            return Err(Error::Protocol(format!(
                "unexpected greeting length {}",
                greeting.len()
            )));
        }
        let frame = wire::client_frame(VENS, command, self.identity.mac, body, [0; 4])?;
        std::io::Write::write_all(&mut stream, &frame)?;
        wire::read_frame(&mut stream, VENS)
    }

    /// Register this host as the target of scanner notifications (control command 0x11).
    pub fn register(
        &self,
        host_ip: Ipv4Addr,
        interface_version: u16,
        password: Option<&str>,
    ) -> Result<()> {
        let now = OffsetDateTime::now_local().map_err(|error| {
            Error::Input(format!(
                "local time zone is unavailable for registration: {error}"
            ))
        })?;
        self.register_at(host_ip, interface_version, password, now)
    }

    pub fn register_at(
        &self,
        host_ip: Ipv4Addr,
        interface_version: u16,
        password: Option<&str>,
        now: OffsetDateTime,
    ) -> Result<()> {
        let body = register_body(&self.identity, host_ip, interface_version, password, now)?;
        let reply = self.call(0x11, &body)?;
        parse_registration_reply(&reply)
    }

    pub fn device_name(&self) -> Result<String> {
        let reply = self.call(0x13, &[])?;
        if reply.len() < 16 {
            return Err(Error::Protocol("short device-name reply".into()));
        }
        let end = reply.len().min(80);
        let name = &reply[16..end];
        let nul = name
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(name.len());
        Ok(String::from_utf8_lossy(&name[..nul]).trim().to_owned())
    }

    pub fn wifi_status(&self) -> Result<u32> {
        // UNVERIFIED: ScanSnack observed status 3 on AC power; the field's meaning remains unknown.
        let reply = self.call(0x30, &[])?;
        if reply.len() < 20 {
            return Err(Error::Protocol("short Wi-Fi status reply".into()));
        }
        Ok(u32::from_be_bytes(reply[16..20].try_into().map_err(
            |_| Error::Protocol("invalid Wi-Fi status field".into()),
        )?))
    }
}

pub fn parse_registration_reply(frame: &[u8]) -> Result<()> {
    if frame.len() < 20 {
        return Err(Error::Protocol(format!(
            "short registration reply: {} bytes",
            frame.len()
        )));
    }
    let status = i32::from_be_bytes(
        frame[8..12]
            .try_into()
            .map_err(|_| Error::Protocol("invalid registration status".into()))?,
    );
    if status == 0 {
        return Ok(());
    }
    let reason = match status {
        -2 => "the scanner requires its Wi-Fi password",
        -3 => "wrong Wi-Fi password",
        -4 => "the scanner is in use by another computer over Wi-Fi or USB",
        -7 => "this host ID is not paired with the scanner; pairing is performed over USB",
        // UNVERIFIED: statuses -1, -5, and -6 are known to exist but their conditions are undocumented.
        _ => "unknown reason",
    };
    Err(Error::Registration {
        status,
        reason: reason.into(),
    })
}

#[cfg(test)]
mod tests {
    use time::{Date, Month, PrimitiveDateTime, Time, UtcOffset};

    use super::*;

    const IDENTITY: HostIdentity = HostIdentity {
        host_id: [1, 2, 3, 4, 5, 6, 7, 8],
        mac: [2, 0, 0, 0, 0, 1],
        notification_port: DEFAULT_NOTIFICATION_PORT,
    };

    fn captured_time() -> OffsetDateTime {
        PrimitiveDateTime::new(
            Date::from_calendar_date(2026, Month::October, 30).unwrap(),
            Time::from_hms(19, 6, 33).unwrap(),
        )
        .assume_offset(UtcOffset::from_hms(-4, 0, 0).unwrap())
    }

    #[test]
    fn password_encoding_matches_scansnack_capture() {
        assert_eq!(encode_password("2345").unwrap(), "173132180179");
        assert_eq!(default_password("A0XX012345").unwrap(), "2345");
        assert!(encode_password("12345678901234567").is_err());
        assert!(encode_password("").is_err());
        assert!(encode_password("pässword").is_err());
        assert!(default_password("A0XX01ABCD").is_err());
    }

    #[test]
    fn registration_body_matches_documented_capture() {
        let body = register_body(
            &IDENTITY,
            Ipv4Addr::new(192, 0, 2, 10),
            6,
            Some("2345"),
            captured_time(),
        )
        .unwrap();
        assert_eq!(body.len(), 96);
        assert_eq!(&body[..12], &[0, 6, 0x1e, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
        assert_eq!(&body[12..16], &[192, 0, 2, 10]);
        assert_eq!(&body[16..20], &55265_u32.to_be_bytes());
        assert_eq!(&body[20..32], b"173132180179");
        assert_eq!(&body[32..68], &[0; 36]);
        assert_eq!(&body[68..76], &[0x07, 0xea, 10, 30, 19, 6, 33, 0]);
        assert_eq!(&body[76..84], &IDENTITY.host_id);
        assert_eq!(&body[84..88], &14400_i32.to_be_bytes());
    }

    #[test]
    fn no_password_or_old_interface_leaves_fields_zero() {
        for password in [None, Some("")] {
            let body = register_body(
                &IDENTITY,
                Ipv4Addr::new(192, 0, 2, 10),
                3,
                password,
                captured_time(),
            )
            .unwrap();
            assert_eq!(&body[8..12], &[0; 4]);
            assert_eq!(&body[20..68], &[0; 48]);
            assert_eq!(&body[84..88], &[0; 4]);
        }
    }

    #[test]
    fn registration_status_is_signed_and_known_errors_are_explained() {
        let mut reply = wire::server_frame(VENS, 0, &[0; 4]).unwrap();
        parse_registration_reply(&reply).unwrap();
        for (status, text) in [
            (-2_i32, "password"),
            (-3, "wrong Wi-Fi password"),
            (-4, "in use"),
            (-7, "not paired"),
            (-1, "unknown reason"),
        ] {
            reply[8..12].copy_from_slice(&status.to_be_bytes());
            let error = parse_registration_reply(&reply).unwrap_err();
            assert!(error.to_string().contains(text));
        }
        assert!(parse_registration_reply(&reply[..19]).is_err());
    }
}
