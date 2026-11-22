//! ScanSnap iX500 UDP discovery using the `VENS` dialect.

use std::collections::BTreeMap;
use std::io;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4, UdpSocket};
use std::time::{Duration, Instant};

use crate::adapters::scansnap_wifi::discovery::{
    DISCOVERY_PORT, LocalNetwork, broadcast_targets, build_probe, local_ip_for, local_ipv4_networks,
};
use crate::adapters::scansnap_wifi::identity::{format_hex, format_mac};
use crate::adapters::scansnap_wifi::wire::VENS;
use crate::core::{Error, FoundScanner, Result, Transport};

pub const PROBE_FLAG: u16 = 0x0010;
pub const REPLY_LEN: usize = 132;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScannerInfo {
    pub ip: Ipv4Addr,
    pub scan_port: u16,
    pub control_port: u16,
    pub mac: [u8; 6],
    pub name: String,
    pub model: String,
    pub interface_version: u16,
    pub paired_ip: Ipv4Addr,
    pub paired_host_id: [u8; 8],
    pub serial: String,
}

impl ScannerInfo {
    #[must_use]
    pub fn found_scanner(&self) -> FoundScanner {
        FoundScanner {
            transport: Transport::Wifi,
            address: self.ip.to_string(),
            name: self.name.clone(),
            model: self.model.clone(),
            serial: self.serial.clone(),
        }
    }

    #[must_use]
    pub fn mac_string(&self) -> String {
        format_mac(self.mac)
    }

    #[must_use]
    pub fn paired_host_id_string(&self) -> String {
        format_hex(&self.paired_host_id)
    }
}

/// Parse a reply, returning `Ok(None)` for another model or a mismatched sender address.
pub fn parse_reply(data: &[u8], sender: Option<Ipv4Addr>) -> Result<Option<ScannerInfo>> {
    if data.len() < REPLY_LEN || data[..4] != VENS {
        return Ok(None);
    }
    let ip = Ipv4Addr::new(data[16], data[17], data[18], data[19]);
    if sender.is_some_and(|address| address != ip) {
        return Ok(None);
    }
    let scan_port = read_port(data, 20)?;
    let control_port = read_port(data, 24)?;
    let name = c_string(&data[40..104]);
    let serial = name
        .rsplit_once('-')
        .map_or(name.as_str(), |(_, serial)| serial)
        .to_owned();
    let mut mac = [0_u8; 6];
    mac.copy_from_slice(&data[28..34]);
    let mut paired_host_id = [0_u8; 8];
    paired_host_id.copy_from_slice(&data[124..132]);

    Ok(Some(ScannerInfo {
        ip,
        scan_port,
        control_port,
        mac,
        name,
        model: c_string(&data[104..120]),
        interface_version: u16::from_be_bytes([data[8], data[9]]),
        paired_ip: Ipv4Addr::new(data[120], data[121], data[122], data[123]),
        paired_host_id,
        serial,
    }))
}

fn read_port(data: &[u8], offset: usize) -> Result<u16> {
    let raw = u32::from_be_bytes(
        data[offset..offset + 4]
            .try_into()
            .map_err(|_| Error::Protocol("truncated discovery port".into()))?,
    );
    u16::try_from(raw).map_err(|_| Error::Protocol(format!("invalid scanner port {raw}")))
}

fn c_string(data: &[u8]) -> String {
    let end = data
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(data.len());
    String::from_utf8_lossy(&data[..end]).trim().to_owned()
}

/// Send one iX500 probe and collect replies until the deadline, deduplicated by scanner IP.
pub fn discover(
    host_mac: [u8; 6],
    target: Ipv4Addr,
    timeout: Duration,
) -> Result<Vec<ScannerInfo>> {
    discover_on_port(host_mac, target, timeout, DISCOVERY_PORT)
}

fn discover_on_port(
    host_mac: [u8; 6],
    target: Ipv4Addr,
    timeout: Duration,
    discovery_port: u16,
) -> Result<Vec<ScannerInfo>> {
    let host_ip = local_ip_for(target)?;
    discover_targets(host_mac, host_ip, &[target], timeout, discovery_port)
}

fn discover_targets(
    host_mac: [u8; 6],
    host_ip: Ipv4Addr,
    targets: &[Ipv4Addr],
    timeout: Duration,
    discovery_port: u16,
) -> Result<Vec<ScannerInfo>> {
    let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))?;
    socket.set_broadcast(true)?;
    let reply_port = socket.local_addr()?.port();
    let probe = build_probe(VENS, PROBE_FLAG, host_ip, host_mac, reply_port, 0);
    for target in targets {
        socket.send_to(&probe, SocketAddrV4::new(*target, discovery_port))?;
    }

    let deadline = Instant::now() + timeout;
    let mut found = BTreeMap::new();
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        socket.set_read_timeout(Some(remaining))?;
        let mut buffer = [0_u8; 2048];
        match socket.recv_from(&mut buffer) {
            Ok((length, sender)) => {
                let sender_ip = match sender {
                    SocketAddr::V4(address) => *address.ip(),
                    SocketAddr::V6(_) => continue,
                };
                if let Some(info) = parse_reply(&buffer[..length], Some(sender_ip))? {
                    found.insert(info.ip, info);
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::TimedOut
                ) =>
            {
                break;
            }
            Err(error) => return Err(error.into()),
        }
    }
    Ok(found.into_values().collect())
}

/// Number of attempts for a unicast discovery. ScanSnap Home spaces its probes about 0.3 s apart, and a
/// single probe or reply can be lost, so retries keep a small delay.
pub const RETRY_SPACING: Duration = Duration::from_millis(300);

pub fn discover_with_retries(
    host_mac: [u8; 6],
    target: Ipv4Addr,
    attempts: u8,
    timeout: Duration,
) -> Result<Vec<ScannerInfo>> {
    let attempts = attempts.max(1);
    let mut last = Vec::new();
    for attempt in 0..attempts {
        if attempt > 0 {
            std::thread::sleep(RETRY_SPACING);
        }
        last = discover(host_mac, target, timeout)?;
        if !last.is_empty() {
            break;
        }
    }
    Ok(last)
}

/// Discover the scanner at `target`.
///
/// A direct unicast probe can go unanswered while the scanner still holds or has just closed a session
/// (docs/protocol-ix500.md, "Discovery and heartbeat"), so this falls back to broadcast and finally to the
/// local-subnet sweep before giving up.
pub fn discover_expected(
    host_mac: [u8; 6],
    target: Ipv4Addr,
    attempts: u8,
    timeout: Duration,
) -> Result<Option<ScannerInfo>> {
    let found = discover_with_retries(host_mac, target, attempts, timeout)?
        .into_iter()
        .find(|info| info.ip == target);
    if found.is_some() {
        return Ok(found);
    }
    Ok(discover_broadcast(host_mac, timeout)?
        .into_iter()
        .find(|info| info.ip == target))
}

/// Discover through every local directed broadcast, then limited broadcast.
pub fn discover_broadcast(host_mac: [u8; 6], timeout: Duration) -> Result<Vec<ScannerInfo>> {
    let mut found = BTreeMap::new();
    let mut last_error = None;
    for target in broadcast_targets()? {
        match discover(host_mac, target, timeout) {
            Ok(infos) => {
                for info in infos {
                    found.insert(info.ip, info);
                }
            }
            Err(error) => last_error = Some(error),
        }
    }
    if !found.is_empty() {
        return Ok(found.into_values().collect());
    }

    // Some iX500 units answer unicast probes but remain silent on both broadcast forms. Probe the local
    // subnet concurrently from one socket so automatic discovery still works without a configured IP.
    match discover_local_subnets(host_mac, timeout) {
        Ok(infos) => {
            for info in infos {
                found.insert(info.ip, info);
            }
        }
        Err(error) => last_error = Some(error),
    }
    if found.is_empty() {
        if let Some(error) = last_error {
            return Err(error);
        }
    }
    Ok(found.into_values().collect())
}

fn discover_local_subnets(host_mac: [u8; 6], timeout: Duration) -> Result<Vec<ScannerInfo>> {
    let mut found = BTreeMap::new();
    for network in local_ipv4_networks()? {
        let targets = subnet_hosts(network);
        if targets.is_empty() {
            continue;
        }
        for info in discover_targets(host_mac, network.address, &targets, timeout, DISCOVERY_PORT)?
        {
            found.insert(info.ip, info);
        }
        if !found.is_empty() {
            break;
        }
    }
    Ok(found.into_values().collect())
}

fn subnet_hosts(network: LocalNetwork) -> Vec<Ipv4Addr> {
    let address = u32::from(network.address);
    let netmask = u32::from(network.netmask);
    let host_bits = netmask.trailing_zeros();
    let (start, end) = if host_bits <= 8 {
        let network_address = address & netmask;
        let broadcast = u32::from(network.broadcast);
        (network_address.saturating_add(1), broadcast)
    } else {
        // Do not sweep a large routed network. Limit the fallback to the interface's local /24.
        let network_address = address & 0xffff_ff00;
        (network_address + 1, network_address + 255)
    };
    (start..end)
        .map(Ipv4Addr::from)
        .filter(|candidate| *candidate != network.address)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOST_ID: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];

    fn reply(ip: Ipv4Addr) -> Vec<u8> {
        let mut data = vec![0_u8; REPLY_LEN];
        data[..4].copy_from_slice(&VENS);
        data[8..12].copy_from_slice(&[0, 6, 0, 0x30]);
        data[16..20].copy_from_slice(&ip.octets());
        data[20..24].copy_from_slice(&53218_u32.to_be_bytes());
        data[24..28].copy_from_slice(&53219_u32.to_be_bytes());
        data[28..34].copy_from_slice(&[0, 0x80, 0x92, 0, 0, 1]);
        data[40..56].copy_from_slice(b"iX500-A0XX012345");
        data[104..120].copy_from_slice(b"ScanSnap iX500  ");
        data[120..124].copy_from_slice(&Ipv4Addr::new(192, 0, 2, 10).octets());
        data[124..132].copy_from_slice(&HOST_ID);
        data
    }

    #[test]
    fn parses_documented_reply_layout() {
        let ip = Ipv4Addr::new(192, 0, 2, 20);
        let info = parse_reply(&reply(ip), Some(ip)).unwrap().unwrap();
        assert_eq!(info.model, "ScanSnap iX500");
        assert_eq!(info.interface_version, 6);
        assert_eq!(info.serial, "A0XX012345");
        assert_eq!(info.scan_port, 53218);
        assert_eq!(info.control_port, 53219);
        assert_eq!(info.paired_ip, Ipv4Addr::new(192, 0, 2, 10));
        assert_eq!(info.paired_host_id, HOST_ID);
        assert_eq!(info.mac_string(), "00:80:92:00:00:01");
        assert_eq!(info.paired_host_id_string(), "0102030405060708");
        assert_eq!(info.found_scanner().transport, Transport::Wifi);
    }

    #[test]
    fn ignores_other_models_and_mismatched_senders() {
        let ip = Ipv4Addr::new(192, 0, 2, 20);
        assert!(
            parse_reply(&reply(ip), Some(Ipv4Addr::new(192, 0, 2, 99)))
                .unwrap()
                .is_none()
        );
        let mut other = reply(ip);
        other[..4].copy_from_slice(b"ssNR");
        assert!(parse_reply(&other, None).unwrap().is_none());
        assert!(parse_reply(&reply(ip)[..100], None).unwrap().is_none());
    }

    #[test]
    fn rejects_impossible_ports() {
        let ip = Ipv4Addr::new(192, 0, 2, 20);
        let mut data = reply(ip);
        data[20..24].copy_from_slice(&65536_u32.to_be_bytes());
        assert!(parse_reply(&data, None).is_err());
    }

    #[test]
    fn subnet_fallback_is_bounded_and_excludes_the_host() {
        let hosts = subnet_hosts(LocalNetwork {
            address: Ipv4Addr::new(192, 0, 2, 10),
            netmask: Ipv4Addr::new(255, 255, 255, 0),
            broadcast: Ipv4Addr::new(192, 0, 2, 255),
        });
        assert_eq!(hosts.len(), 253);
        assert_eq!(hosts.first(), Some(&Ipv4Addr::new(192, 0, 2, 1)));
        assert_eq!(hosts.last(), Some(&Ipv4Addr::new(192, 0, 2, 254)));
        assert!(!hosts.contains(&Ipv4Addr::new(192, 0, 2, 10)));

        let large = subnet_hosts(LocalNetwork {
            address: Ipv4Addr::new(192, 0, 2, 10),
            netmask: Ipv4Addr::new(255, 255, 0, 0),
            broadcast: Ipv4Addr::new(192, 0, 255, 255),
        });
        assert_eq!(large.len(), 253);
        assert_eq!(large.last(), Some(&Ipv4Addr::new(192, 0, 2, 254)));
    }

    #[test]
    fn exchanges_a_real_udp_probe_with_a_mock_scanner() {
        let scanner = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = scanner.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let mut buffer = [0_u8; 64];
            let (length, host) = scanner.recv_from(&mut buffer).unwrap();
            assert_eq!(length, 32);
            assert_eq!(&buffer[..4], b"VENS");
            assert_eq!(&buffer[24..26], &PROBE_FLAG.to_be_bytes());
            scanner.send_to(&reply(Ipv4Addr::LOCALHOST), host).unwrap();
        });

        let found = discover_on_port(
            [2, 0, 0, 0, 0, 1],
            Ipv4Addr::LOCALHOST,
            Duration::from_secs(1),
            port,
        )
        .unwrap();
        server.join().unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].serial, "A0XX012345");
    }
}
