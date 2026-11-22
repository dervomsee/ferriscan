//! ScanSnap iX500 Wi-Fi adapter (`VENS` protocol dialect).

pub mod control;
pub mod device;
pub mod discovery;
pub mod identity;
mod locator;
pub mod scanner;
pub mod scsi;

pub use device::Ix500Scanner;
pub use locator::Ix500Locator;
