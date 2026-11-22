//! iX500 scan sequence and captured parameter blocks.

use std::net::Ipv4Addr;
use std::time::Duration;

use super::scsi::{CHECK_CONDITION, ScsiChannel};
use crate::adapters::scansnap_wifi::scsi::scsi_error;
use crate::core::{
    ColorMode, Error, ImageInfo, MultifeedDetection, PageSize, Result, ScanSettings, ScanSummary,
    Sheet,
};

pub const READ_LEN: usize = 0x30_0000;
pub const MAX_IMAGE: usize = 64 << 20;
pub const DPIS: [u16; 4] = [150, 200, 300, 600];

pub fn check_dpi(dpi: u16) -> Result<u16> {
    if DPIS.contains(&dpi) {
        Ok(dpi)
    } else {
        Err(Error::Unsupported(format!(
            "dpi must be one of {}, got {dpi}",
            DPIS.map(|value| value.to_string()).join("/")
        )))
    }
}

#[must_use]
pub fn page_dimensions(page: PageSize) -> (u32, u32) {
    match page {
        PageSize::Auto => (0x28d0, 0x44dc),
        PageSize::A4 => (0x26c1, 0x36d0),
        PageSize::Letter => (0x27d8, 0x3390),
        PageSize::Legal => (0x27d8, 0x41a0),
        PageSize::B5 => (0x2196, 0x2f6e),
        PageSize::A5 => (0x1b50, 0x26c1),
        PageSize::B6 => (0x179f, 0x2196),
    }
}

/// Build the 32-byte E9 window block.
pub fn window_block(dpi: u16, page: PageSize) -> Result<[u8; 32]> {
    check_dpi(dpi)?;
    let (width, height) = page_dimensions(page);
    let mut block = [0_u8; 32];
    block[..2].copy_from_slice(&dpi.to_be_bytes());
    block[2..4].copy_from_slice(&dpi.to_be_bytes());
    block[4..8].copy_from_slice(&width.to_be_bytes());
    block[8..12].copy_from_slice(&height.to_be_bytes());
    block[12] = 5;
    Ok(block)
}

/// Build the 80-byte D4 parameter and window-descriptor block.
pub fn mode_block(dpi: u16, page: PageSize, multifeed: MultifeedDetection) -> Result<[u8; 80]> {
    check_dpi(dpi)?;
    let (width, height) = page_dimensions(page);
    let mut block = [0_u8; 80];
    block[1] = 3;
    if page == PageSize::Auto {
        block[2] = 1;
        block[3] = 1;
    }
    block[4] = match multifeed {
        MultifeedDetection::Off => 0,
        MultifeedDetection::Ultrasonic => 0xd0,
    };
    block[6] = 0xc1;
    block[7..10].fill(0x80);
    block[10] = if dpi == 300 { 0x90 } else { 0x80 };
    block[11] = 0x80;
    block[12] = 0x80;
    block[13] = if dpi == 600 { 0x80 } else { 0 };
    block[31] = 0x30;
    block[32] = 0;
    block[33] = 0x10;
    block[34..36].copy_from_slice(&dpi.to_be_bytes());
    block[36..38].copy_from_slice(&dpi.to_be_bytes());
    block[38] = 5;
    block[39] = 0x81;
    block[40] = match dpi {
        150 | 200 => 5,
        300 => 0,
        600 => 0x0a,
        _ => return Err(Error::Input("unsupported dpi".into())),
    };
    block[42..46].copy_from_slice(&width.to_be_bytes());
    block[46..50].copy_from_slice(&height.to_be_bytes());
    block[50] = 4;
    Ok(block)
}

/// Parse the image-info block. The scanner returns 0x20 bytes; at least 22 are required to read every
/// field this function exposes.
pub fn parse_image_info(data: &[u8]) -> Result<ImageInfo> {
    if data.len() < 22 {
        return Err(Error::Protocol(format!(
            "short image info: {} bytes",
            data.len()
        )));
    }
    Ok(ImageInfo {
        width: be_u32(data, 0)?,
        max_height: be_u32(data, 4)?,
        height: be_u32(data, 12)?,
        // Live-validated on one iX500: at 300 dpi both fields read 0x012c and at 600 dpi both read 0x0258,
        // matching the requested resolution. The inherited ScanSnack layout put them two bytes earlier,
        // which reads 0 for dpi_x. See docs/protocol-ix500.md, section "Image info".
        dpi_x: be_u16(data, 18)?,
        dpi_y: be_u16(data, 20)?,
        raw: data[..data.len().min(32)].to_vec(),
    })
}

fn be_u32(data: &[u8], offset: usize) -> Result<u32> {
    Ok(u32::from_be_bytes(
        data[offset..offset + 4]
            .try_into()
            .map_err(|_| Error::Protocol("invalid 32-bit image-info field".into()))?,
    ))
}

fn be_u16(data: &[u8], offset: usize) -> Result<u16> {
    Ok(u16::from_be_bytes(
        data[offset..offset + 2]
            .try_into()
            .map_err(|_| Error::Protocol("invalid 16-bit image-info field".into()))?,
    ))
}

fn read_cdb(back: bool, index: u8) -> [u8; 12] {
    let mut cdb = [0_u8; 12];
    cdb[0] = 0x28;
    cdb[3] = 2;
    cdb[5] = if back { 0x80 } else { 0 };
    cdb[6] = 0x30; // READ_LEN = 0x300000
    cdb[7] = 0;
    cdb[8] = 0;
    cdb[10] = index;
    cdb
}

fn info_cdb(index: u8) -> [u8; 12] {
    [0x28, 0, 0x80, 0, 0, 0x80, 0, 0, 0x20, 0, index, 0]
}

pub struct Scanner<'a> {
    channel: &'a mut ScsiChannel,
}

impl<'a> Scanner<'a> {
    pub fn new(channel: &'a mut ScsiChannel) -> Self {
        Self { channel }
    }

    fn read_image(&mut self, back: bool, index: u8) -> Result<Vec<u8>> {
        let mut image = Vec::new();
        for chunk in 0..=u8::MAX {
            let response = self
                .channel
                .command(&read_cdb(back, index), &[], READ_LEN)?;
            let sense = if response.status == CHECK_CONDITION {
                Some(self.channel.request_sense()?)
            } else {
                None
            };
            if let Some(sense) = sense.filter(|sense| !sense.eom) {
                return Err(scsi_error("image read failed", sense));
            }
            if response.status != 0 && response.status != CHECK_CONDITION {
                return Err(Error::Scsi(format!(
                    "image read returned status {}",
                    response.status
                )));
            }
            if response.data.len() < 2 {
                return Err(Error::Protocol("short image chunk header".into()));
            }
            if response.data[..2] != [index, chunk] {
                return Err(Error::Protocol(format!(
                    "unexpected image header {:02x?} for image {index}, chunk {chunk}",
                    &response.data[..2]
                )));
            }
            image.extend_from_slice(&response.data[2..]);
            if image.len() > MAX_IMAGE {
                return Err(Error::Scsi(format!(
                    "image {index} exceeds {} MiB without an end",
                    MAX_IMAGE >> 20
                )));
            }
            if sense.is_some() {
                return Ok(image);
            }
            // UNVERIFIED: images larger than READ_LEN repeat the CDB; ScanSnack never observed this live.
        }
        Err(Error::Scsi(format!(
            "image {index} has more than 256 chunks"
        )))
    }

    /// Scan all sheets. The iX500 always acquires both sides and produces color JPEG data.
    pub fn scan(
        &mut self,
        settings: &ScanSettings,
        on_sheet: &mut dyn FnMut(Sheet) -> Result<()>,
    ) -> Result<ScanSummary> {
        if settings.color_mode == ColorMode::Gray {
            return Err(Error::Unsupported(
                "iX500 grayscale conversion is not implemented yet; use color mode".into(),
            ));
        }
        check_dpi(settings.dpi)?;
        self.channel.hardware_status()?;
        self.channel.checked(&[0xd5, 0, 0, 0, 8, 8], &[0; 8], 8)?;
        self.channel.checked(&[0xd8, 0, 0, 0, 0, 0], &[], 0)?;
        self.channel.checked(
            &[0xe9, 0, 0, 0, 0, 0, 0, 0x20, 0, 0],
            &window_block(settings.dpi, settings.page_size)?,
            0,
        )?;
        self.channel.checked(
            &[0xd4, 0, 0, 0, 0x50, 0],
            &mode_block(settings.dpi, settings.page_size, settings.multifeed)?,
            0,
        )?;
        self.channel.hardware_status()?;
        self.channel.request_sense()?;

        // UNVERIFIED: the one-byte image index wraps after 128 sheets in one job.
        let mut index = 0_u8;
        let mut summary = ScanSummary::default();
        loop {
            let feed = self.channel.command(&[0xe0, 0, 0, 0, 0, 0], &[], 0)?;
            if feed.status == CHECK_CONDITION {
                let sense = self.channel.request_sense()?;
                if sense.no_paper() {
                    if summary.sheets == 0 {
                        return Err(Error::NoPaper);
                    }
                    return Ok(summary);
                }
                return Err(scsi_error("paper feed failed", sense));
            }
            if feed.status != 0 {
                return Err(Error::Scsi(format!("feed status {}", feed.status)));
            }

            let front = self.read_image(false, index)?;
            let back_index = index.wrapping_add(1);
            let back = self.read_image(true, back_index)?;
            let info =
                parse_image_info(&self.channel.checked(&info_cdb(back_index), &[], 0x20)?.data)?;
            self.channel.hardware_status()?;
            on_sheet(Sheet {
                front,
                back: settings.duplex.then_some(back),
                info: Some(info),
            })?;
            index = index.wrapping_add(2);
            summary.sheets += 1;
            summary.images += 1 + u32::from(settings.duplex);
        }
    }
}

/// Release a scan session with D6 on a new connection, as observed by ScanSnack.
pub fn release(ip: Ipv4Addr, mac: [u8; 6], port: u16) -> Result<()> {
    let mut channel = ScsiChannel::connect(ip, mac, port, Duration::from_secs(10))?;
    channel.checked(&[0xd6, 0, 0, 0, 0, 0], &[], 0)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parameter_blocks_match_scansnack_captures() {
        let expected_window = hex::decode("012c012c000028d0000044dc05").unwrap();
        assert_eq!(
            &window_block(300, PageSize::Auto).unwrap()[..13],
            &expected_window
        );
        let expected_mode = hex::decode(
            "00030101d000c1808080908080000000000000000000000000000000000000300010012c012c05810000000028d0000044dc04",
        )
        .unwrap();
        assert_eq!(
            &mode_block(300, PageSize::Auto, MultifeedDetection::Ultrasonic).unwrap()[..51],
            &expected_mode
        );
    }

    #[test]
    fn page_and_dpi_variants_change_only_documented_fields() {
        let block = mode_block(600, PageSize::A4, MultifeedDetection::Off).unwrap();
        assert_eq!((block[2], block[3], block[4]), (0, 0, 0));
        assert_eq!((block[10], block[13], block[40]), (0x80, 0x80, 0x0a));
        assert_eq!(&block[42..46], &0x26c1_u32.to_be_bytes());
        assert_eq!(&block[46..50], &0x36d0_u32.to_be_bytes());
        assert!(window_block(250, PageSize::Auto).is_err());
    }

    // Raw image-info blocks captured from a live iX500 (docs/protocol-ix500.md, section "Image info").
    const LIVE_INFO_300: &str = "00000a300000113000000a3000000e500000012c012c00000a3400000e550000";
    const LIVE_INFO_600: &str = "00001468000022680000146800001c800000025802580000146800001c870000";

    #[test]
    fn image_info_matches_live_captures() {
        let info = parse_image_info(&hex::decode(LIVE_INFO_300).unwrap()).unwrap();
        assert_eq!(
            (info.width, info.max_height, info.height),
            (2608, 4400, 3664)
        );
        assert_eq!((info.dpi_x, info.dpi_y), (300, 300));
        assert_eq!(info.raw.len(), 32);

        let info = parse_image_info(&hex::decode(LIVE_INFO_600).unwrap()).unwrap();
        assert_eq!((info.width, info.height), (5224, 7296));
        assert_eq!((info.dpi_x, info.dpi_y), (600, 600));

        // The inherited ScanSnack fixture stores two equal 300 values at bytes 16 and 18, so it cannot
        // distinguish the layouts: read with the live offsets, its dpi_y stays zero.
        let inherited =
            hex::decode("00000a300000113000000a3000000d90012c012c00000a3400000d9600000000")
                .unwrap();
        let inherited = parse_image_info(&inherited).unwrap();
        assert_eq!((inherited.dpi_x, inherited.dpi_y), (300, 0));
        assert!(parse_image_info(&[0_u8; 21]).is_err());
    }
}
