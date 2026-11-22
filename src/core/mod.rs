//! Transport-independent scanner domain types and errors.

mod error;
mod model;

pub use error::{Error, Result};
pub use model::{
    ColorMode, FoundScanner, ImageInfo, MultifeedDetection, PageSize, ScanSettings, Sheet,
    Transport,
};
