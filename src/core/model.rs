/// Connection technology used to reach a scanner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Transport {
    Wifi,
    /// Reserved for a future adapter; no USB support is currently claimed.
    Usb,
}

/// A discovered scanner with a transport-specific address represented as text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FoundScanner {
    pub transport: Transport,
    pub address: String,
    pub name: String,
    pub model: String,
    pub serial: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    Color,
    Gray,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageSize {
    Auto,
    A4,
    Letter,
    Legal,
    B5,
    A5,
    B6,
}

impl std::str::FromStr for PageSize {
    type Err = String;

    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "a4" => Ok(Self::A4),
            "letter" => Ok(Self::Letter),
            "legal" => Ok(Self::Legal),
            "b5" => Ok(Self::B5),
            "a5" => Ok(Self::A5),
            "b6" => Ok(Self::B6),
            _ => Err(format!(
                "unknown page size '{value}'; expected auto, a4, letter, legal, b5, a5, or b6"
            )),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MultifeedDetection {
    Off,
    Ultrasonic,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanSettings {
    pub color_mode: ColorMode,
    pub dpi: u16,
    pub duplex: bool,
    pub page_size: PageSize,
    pub multifeed: MultifeedDetection,
}

impl Default for ScanSettings {
    fn default() -> Self {
        Self {
            color_mode: ColorMode::Color,
            dpi: 300,
            duplex: true,
            page_size: PageSize::Auto,
            multifeed: MultifeedDetection::Ultrasonic,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageInfo {
    pub width: u32,
    pub max_height: u32,
    pub height: u32,
    pub dpi_x: u16,
    pub dpi_y: u16,
    /// Raw scanner response the fields were derived from. Kept for diagnostics because the resolution
    /// field layout is not fully verified.
    pub raw: Vec<u8>,
}

/// One fed sheet, with JPEG data as supplied by the scanner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sheet {
    pub front: Vec<u8>,
    pub back: Option<Vec<u8>>,
    pub info: Option<ImageInfo>,
}

/// Which side of a fed sheet an image belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageSide {
    Front,
    Back,
}

/// Outcome of one completed scan run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ScanSummary {
    /// Sheets pulled through the feeder and delivered to the caller.
    pub sheets: u32,
    /// Images delivered: fronts plus, in duplex mode, backs.
    pub images: u32,
}
