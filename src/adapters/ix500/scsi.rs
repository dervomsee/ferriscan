//! iX500-specific configuration of the shared SCSI tunnel.

use std::net::Ipv4Addr;
use std::time::Duration;

use crate::adapters::scansnap_wifi::scsi::{Response, ScsiChannel as SharedChannel, Sense};
use crate::adapters::scansnap_wifi::wire::VENS;
use crate::core::Result;

pub const C2_LEN: u8 = 0x20;
pub const DATA_PORT: u16 = crate::adapters::scansnap_wifi::scsi::DATA_PORT;
pub const CHECK_CONDITION: u32 = crate::adapters::scansnap_wifi::scsi::CHECK_CONDITION;

pub struct ScsiChannel {
    inner: SharedChannel,
}

impl ScsiChannel {
    pub fn connect(ip: Ipv4Addr, mac: [u8; 6], port: u16, timeout: Duration) -> Result<Self> {
        // The iX500 sends zero in the extra field, unlike the iX1500's host-IP field.
        let inner = SharedChannel::connect(ip, port, mac, VENS, [0; 4], timeout)?;
        Ok(Self { inner })
    }

    pub fn command(&mut self, cdb: &[u8], out: &[u8], in_len: usize) -> Result<Response> {
        self.inner.command(cdb, out, in_len)
    }

    pub fn checked(&mut self, cdb: &[u8], out: &[u8], in_len: usize) -> Result<Response> {
        self.inner.checked(cdb, out, in_len)
    }

    pub fn request_sense(&mut self) -> Result<Sense> {
        self.inner.request_sense()
    }

    pub fn hardware_status(&mut self) -> Result<Vec<u8>> {
        self.inner.hardware_status(C2_LEN)
    }

    pub fn inquiry(&mut self) -> Result<(String, String, String)> {
        self.inner.inquiry()
    }
}
