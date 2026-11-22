//! UDP discovery primitives shared by ScanSnap Wi-Fi models.

use std::collections::{BTreeMap, BTreeSet};
use std::io;
use std::net::{Ipv4Addr, SocketAddrV4, UdpSocket};

use nix::ifaddrs::getifaddrs;

use crate::core::Result;

pub const DISCOVERY_PORT: u16 = 52217;
pub const HEARTBEAT_FLAG: u16 = 0x1000;
pub const BROADCAST: Ipv4Addr = Ipv4Addr::BROADCAST;
pub const PROBE_LEN: usize = 32;

#[must_use]
pub fn build_probe(
    magic: [u8; 4],
    flag: u16,
    host_ip: Ipv4Addr,
    host_mac: [u8; 6],
    reply_port: u16,
    sequence: u32,
) -> [u8; PROBE_LEN] {
    let mut probe = [0_u8; PROBE_LEN];
    probe[..4].copy_from_slice(&magic);
    probe[4..8].copy_from_slice(&sequence.to_be_bytes());
    probe[8..12].copy_from_slice(&host_ip.octets());
    probe[12..18].copy_from_slice(&host_mac);
    probe[22..24].copy_from_slice(&reply_port.to_be_bytes());
    probe[24..26].copy_from_slice(&flag.to_be_bytes());
    probe
}

/// Find the local IPv4 address selected by the routing table for a target.
pub fn local_ip_for(target: Ipv4Addr) -> Result<Ipv4Addr> {
    let route_target = if target == BROADCAST {
        Ipv4Addr::new(8, 8, 8, 8)
    } else {
        target
    };
    let socket = UdpSocket::bind(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0))?;
    // connect() also rejects broadcast destinations unless SO_BROADCAST is set first.
    socket.set_broadcast(true)?;
    socket.connect(SocketAddrV4::new(route_target, 9))?;
    match socket.local_addr()? {
        std::net::SocketAddr::V4(address) => Ok(*address.ip()),
        std::net::SocketAddr::V6(_) => Err(io::Error::new(
            io::ErrorKind::AddrNotAvailable,
            "scanner route selected an IPv6 address",
        )
        .into()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalNetwork {
    pub address: Ipv4Addr,
    pub netmask: Ipv4Addr,
    pub broadcast: Ipv4Addr,
}

/// Return local IPv4 interface networks with usable broadcast and netmask fields.
pub fn local_ipv4_networks() -> Result<Vec<LocalNetwork>> {
    let interfaces = getifaddrs().map_err(|errno| io::Error::from_raw_os_error(errno as i32))?;
    let mut networks = BTreeMap::new();
    for interface in interfaces {
        let (Some(address), Some(netmask), Some(broadcast)) =
            (interface.address, interface.netmask, interface.broadcast)
        else {
            continue;
        };
        let (Some(address), Some(netmask), Some(broadcast)) = (
            address.as_sockaddr_in(),
            netmask.as_sockaddr_in(),
            broadcast.as_sockaddr_in(),
        ) else {
            continue;
        };
        let network = LocalNetwork {
            address: address.ip(),
            netmask: netmask.ip(),
            broadcast: broadcast.ip(),
        };
        if network.address != Ipv4Addr::UNSPECIFIED && network.broadcast != Ipv4Addr::UNSPECIFIED {
            networks.insert((network.address, network.netmask), network);
        }
    }
    Ok(networks.into_values().collect())
}

/// Return directed IPv4 broadcast addresses for local interfaces, followed by limited broadcast.
pub fn broadcast_targets() -> Result<Vec<Ipv4Addr>> {
    let mut targets = local_ipv4_networks()?
        .into_iter()
        .map(|network| network.broadcast)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    targets.push(BROADCAST);
    Ok(targets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_uses_documented_big_endian_offsets() {
        let probe = build_probe(
            *b"VENS",
            0x0010,
            Ipv4Addr::new(192, 0, 2, 10),
            [2, 0, 0, 0, 0, 1],
            55264,
            1,
        );
        assert_eq!(&probe[..8], b"VENS\0\0\0\x01");
        assert_eq!(&probe[8..12], &[192, 0, 2, 10]);
        assert_eq!(&probe[12..18], &[2, 0, 0, 0, 0, 1]);
        assert_eq!(&probe[18..22], &[0; 4]);
        assert_eq!(&probe[22..26], &[0xd7, 0xe0, 0x00, 0x10]);
        assert_eq!(&probe[26..], &[0; 6]);
    }
}
