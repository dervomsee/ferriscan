//! Application-facing ports independent of transport and scanner model.

mod output;
mod scanner;

pub use output::{ScanOutput, WrittenPage};
pub use scanner::{ScannerLocator, ScannerPort};
