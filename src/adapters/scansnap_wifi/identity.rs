//! Local interface identity used in ScanSnap Wi-Fi frames.

use std::fs;
use std::io;
use std::net::Ipv4Addr;

use nix::ifaddrs::getifaddrs;

use super::discovery::local_ip_for;
use crate::core::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WifiIdentity {
    pub host_ip: Ipv4Addr,
    pub mac: [u8; 6],
}

pub fn local_identity_for(target: Ipv4Addr) -> Result<WifiIdentity> {
    let host_ip = local_ip_for(target)?;
    let mac = interface_mac(host_ip)?;
    Ok(WifiIdentity { host_ip, mac })
}

/// Return the MAC address of the interface owning `host_ip`.
///
/// The iX500 uses this address as its scan-session identity, so selecting an arbitrary host interface is
/// not sufficient. Linux is implemented first; other platforms need their own adapter implementation.
#[cfg(target_os = "linux")]
pub fn interface_mac(host_ip: Ipv4Addr) -> Result<[u8; 6]> {
    let interfaces = getifaddrs().map_err(|errno| io::Error::from_raw_os_error(errno as i32))?;
    for interface in interfaces {
        let Some(address) = interface.address else {
            continue;
        };
        let matches = address
            .as_sockaddr_in()
            .is_some_and(|ipv4| ipv4.ip() == host_ip);
        if !matches {
            continue;
        }
        if let Some(link) = address.as_link_addr() {
            if let Some(mac) = link.addr() {
                if mac != [0; 6] {
                    return Ok(mac);
                }
            }
        }

        // An AF_INET getifaddrs entry does not necessarily expose its AF_PACKET link address. Use the
        // interface name from the matching address entry and ask Linux sysfs for the hardware address.
        let path = format!("/sys/class/net/{}/address", interface.interface_name);
        if let Ok(value) = fs::read_to_string(path) {
            let mac = parse_mac(value.trim())?;
            if mac != [0; 6] {
                return Ok(mac);
            }
        }
    }
    Err(Error::Input(format!(
        "no non-zero MAC address found for local interface with address {host_ip}"
    )))
}

#[cfg(not(target_os = "linux"))]
pub fn interface_mac(_host_ip: Ipv4Addr) -> Result<[u8; 6]> {
    Err(Error::Input(
        "automatic interface MAC lookup is currently implemented only on Linux; use --mac".into(),
    ))
}

pub fn parse_mac(value: &str) -> Result<[u8; 6]> {
    let normalized = value.replace([':', '-'], "");
    if normalized.len() != 12 || !normalized.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(Error::Input(
            "MAC address must contain six hexadecimal octets".into(),
        ));
    }
    let mut mac = [0_u8; 6];
    for (index, octet) in mac.iter_mut().enumerate() {
        *octet = u8::from_str_radix(&normalized[index * 2..index * 2 + 2], 16)
            .map_err(|_| Error::Input("invalid MAC address".into()))?;
    }
    Ok(mac)
}

/// Lowercase hexadecimal representation, without separators.
#[must_use]
pub fn format_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        encoded.push(char::from(DIGITS[usize::from(byte >> 4)]));
        encoded.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    encoded
}

#[must_use]
pub fn format_mac(mac: [u8; 6]) -> String {
    mac.iter()
        .map(|octet| format!("{octet:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_formats_mac_addresses() {
        let mac = parse_mac("02:00:00:00:00:01").unwrap();
        assert_eq!(mac, [2, 0, 0, 0, 0, 1]);
        assert_eq!(parse_mac("02-00-00-00-00-01").unwrap(), mac);
        assert_eq!(format_mac(mac), "02:00:00:00:00:01");
        assert!(parse_mac("02:00:00").is_err());
        assert!(parse_mac("gg:00:00:00:00:01").is_err());
    }
}
