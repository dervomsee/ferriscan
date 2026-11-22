use std::time::Duration;

use crate::core::{FoundScanner, Result, ScanSettings, ScanSummary, Sheet};

/// A connected scanner. Implementations may block and must not run on a GUI thread.
pub trait ScannerPort {
    fn connect(&mut self) -> Result<()>;
    fn disconnect(&mut self) -> Result<()>;

    /// Scan all available sheets and deliver each one as soon as it is complete. Returns how many
    /// sheets and images were delivered.
    fn scan(
        &mut self,
        settings: &ScanSettings,
        on_sheet: &mut dyn FnMut(Sheet) -> Result<()>,
    ) -> Result<ScanSummary>;
}

/// Discovery boundary implemented independently by each transport.
pub trait ScannerLocator {
    /// Discover scanners, optionally restricting the search to a transport-specific target.
    fn discover(&self, target: Option<&str>, timeout: Duration) -> Result<Vec<FoundScanner>>;
}
