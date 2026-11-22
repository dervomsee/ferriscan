use std::net::Ipv4Addr;
use std::time::Duration;

use super::discovery::{discover_broadcast, discover_with_retries};
use crate::core::{Error, FoundScanner, Result};
use crate::ports::ScannerLocator;

#[derive(Debug, Clone, Copy)]
pub struct Ix500Locator {
    host_mac: [u8; 6],
}

impl Ix500Locator {
    #[must_use]
    pub fn new(host_mac: [u8; 6]) -> Self {
        Self { host_mac }
    }
}

impl ScannerLocator for Ix500Locator {
    fn discover(&self, target: Option<&str>, timeout: Duration) -> Result<Vec<FoundScanner>> {
        let found = match target {
            Some(value) => {
                let target = value.parse::<Ipv4Addr>().map_err(|error| {
                    Error::Input(format!("invalid iX500 IPv4 target '{value}': {error}"))
                })?;
                discover_with_retries(self.host_mac, target, 3, timeout)?
            }
            None => discover_broadcast(self.host_mac, timeout)?,
        };
        Ok(found
            .iter()
            .map(super::discovery::ScannerInfo::found_scanner)
            .collect())
    }
}
