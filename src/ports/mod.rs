//! Application-facing ports independent of transport and scanner model.

mod scanner;

pub use scanner::{ScannerLocator, ScannerPort};
