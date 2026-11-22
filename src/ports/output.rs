//! Output boundary for persisting scanned pages.

use std::path::PathBuf;

use crate::core::{PageSide, Result, Sheet};

/// One file written by a scan output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrittenPage {
    pub side: PageSide,
    pub path: PathBuf,
}

/// Persists scanned sheets. Implementations own file naming, durability, and overwrite policy, and
/// therefore do any filesystem or network work off the GUI thread.
pub trait ScanOutput {
    /// Persist one scanned sheet. `index` is 1-based and identifies the sheet within the run.
    fn write(&mut self, index: u32, sheet: &Sheet) -> Result<Vec<WrittenPage>>;
}
