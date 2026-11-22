//! SCSI commands tunnelled through the shared ScanSnap Wi-Fi scan channel.

use std::fmt;
use std::io::Write;
use std::net::{Ipv4Addr, SocketAddrV4, TcpStream};
use std::time::Duration;

use super::wire;
use crate::core::{Error, Result};

pub const DATA_PORT: u16 = 53218;
pub const CHECK_CONDITION: u32 = 2;
pub const RESPONSE_HEADER_LEN: usize = 40;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sense {
    pub key: u8,
    pub asc: u8,
    pub ascq: u8,
    pub eom: bool,
    /// Residual length for an end-of-image response.
    pub info: u32,
}

impl Sense {
    pub fn parse(data: &[u8]) -> Result<Self> {
        if data.len() < 14 {
            return Err(Error::Protocol(format!(
                "short sense data: {} bytes",
                data.len()
            )));
        }
        Ok(Self {
            key: data[2] & 0x0f,
            asc: data[12],
            ascq: data[13],
            eom: data[2] & 0x40 != 0,
            info: u32::from_be_bytes(
                data[3..7]
                    .try_into()
                    .map_err(|_| Error::Protocol("invalid sense information".into()))?,
            ),
        })
    }

    #[must_use]
    pub fn no_paper(self) -> bool {
        (self.key, self.asc, self.ascq) == (3, 0x80, 0x03)
    }

    #[must_use]
    pub fn medium_error(self) -> Option<String> {
        if (self.key, self.asc) != (3, 0x80) {
            return None;
        }
        let reason = match self.ascq {
            0x01 => "paper jam".into(),
            0x03 => "no paper".into(),
            // UNVERIFIED: these labels come from ScanSnack's SANE cross-check, not live observations.
            0x02 => "cover open".into(),
            0x07 => "double feed".into(),
            other => format!("medium error 0x{other:02x}"),
        };
        Some(reason)
    }
}

impl fmt::Display for Sense {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "sense key=0x{:x} asc=0x{:02x} ascq=0x{:02x} eom={}",
            self.key, self.asc, self.ascq, self.eom
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u32,
    pub data: Vec<u8>,
}

pub struct ScsiChannel {
    stream: TcpStream,
    mac: [u8; 6],
    magic: [u8; 4],
    extra: [u8; 4],
}

impl ScsiChannel {
    pub fn connect(
        ip: Ipv4Addr,
        port: u16,
        mac: [u8; 6],
        magic: [u8; 4],
        extra: [u8; 4],
        timeout: Duration,
    ) -> Result<Self> {
        let mut stream = TcpStream::connect_timeout(&SocketAddrV4::new(ip, port).into(), timeout)?;
        stream.set_read_timeout(Some(timeout))?;
        stream.set_write_timeout(Some(timeout))?;
        let greeting = wire::read_frame(&mut stream, magic)?;
        if greeting.len() != wire::SERVER_HEADER_LEN {
            return Err(Error::Protocol(format!(
                "unexpected scan-channel greeting length {}",
                greeting.len()
            )));
        }
        Ok(Self {
            stream,
            mac,
            magic,
            extra,
        })
    }

    pub fn command(&mut self, cdb: &[u8], out: &[u8], in_len: usize) -> Result<Response> {
        if !(6..=16).contains(&cdb.len()) {
            return Err(Error::Input(format!(
                "CDB length must be 6 to 16 bytes, got {}",
                cdb.len()
            )));
        }
        let cdb_len = u32::try_from(cdb.len())
            .map_err(|_| Error::Input("CDB length does not fit in u32".into()))?;
        let in_len_u32 = u32::try_from(in_len)
            .map_err(|_| Error::Input("expected data length does not fit in u32".into()))?;
        let out_len = u32::try_from(out.len())
            .map_err(|_| Error::Input("output data length does not fit in u32".into()))?;
        let mut body = Vec::with_capacity(32 + out.len());
        body.extend_from_slice(&cdb_len.to_be_bytes());
        body.extend_from_slice(&in_len_u32.to_be_bytes());
        body.extend_from_slice(&out_len.to_be_bytes());
        body.extend_from_slice(&0_u32.to_be_bytes());
        body.extend_from_slice(cdb);
        body.resize(32, 0);
        body.extend_from_slice(out);
        let frame = wire::client_frame(self.magic, 1, self.mac, &body, self.extra)?;
        self.stream.write_all(&frame)?;
        let reply = wire::read_frame(&mut self.stream, self.magic)?;
        if reply.len() < RESPONSE_HEADER_LEN {
            return Err(Error::Protocol(format!(
                "short SCSI reply: {} bytes",
                reply.len()
            )));
        }
        let data = reply[RESPONSE_HEADER_LEN..].to_vec();
        if data.len() > in_len {
            return Err(Error::Protocol(format!(
                "SCSI reply contains {} bytes for a read of {in_len}",
                data.len()
            )));
        }
        Ok(Response {
            status: wire::frame_status(&reply)?,
            data,
        })
    }

    pub fn request_sense(&mut self) -> Result<Sense> {
        let response = self.command(&[0x03, 0, 0, 0, 0x12, 0], &[], 0x12)?;
        if response.status != 0 {
            return Err(Error::Scsi(format!(
                "REQUEST SENSE returned status {}",
                response.status
            )));
        }
        Sense::parse(&response.data)
    }

    pub fn checked(&mut self, cdb: &[u8], out: &[u8], in_len: usize) -> Result<Response> {
        let response = self.command(cdb, out, in_len)?;
        if response.status == CHECK_CONDITION {
            let sense = self.request_sense()?;
            return Err(scsi_error(
                &format!("command 0x{:02x} failed", cdb[0]),
                sense,
            ));
        }
        if response.status != 0 {
            return Err(Error::Scsi(format!(
                "command 0x{:02x} returned status {}",
                cdb[0], response.status
            )));
        }
        Ok(response)
    }

    pub fn hardware_status(&mut self, length: u8) -> Result<Vec<u8>> {
        let cdb = [0xc2, 0, 0, 0, 0, 0, 0, 0, length, 0];
        let data = self.checked(&cdb, &[], usize::from(length))?.data;
        if data.len() < 8 {
            return Err(Error::Protocol(format!(
                "short C2 status: {} bytes",
                data.len()
            )));
        }
        Ok(data)
    }

    pub fn inquiry(&mut self) -> Result<(String, String, String)> {
        let data = self.checked(&[0x12, 0, 0, 0, 0x60, 0], &[], 0x60)?.data;
        if data.len() < 36 {
            return Err(Error::Protocol(format!(
                "short INQUIRY data: {} bytes",
                data.len()
            )));
        }
        Ok((
            String::from_utf8_lossy(&data[8..16]).trim().to_owned(),
            String::from_utf8_lossy(&data[16..32]).trim().to_owned(),
            String::from_utf8_lossy(&data[32..36]).trim().to_owned(),
        ))
    }
}

#[must_use]
pub fn scsi_error(message: &str, sense: Sense) -> Error {
    sense.medium_error().map_or_else(
        || Error::Scsi(format!("{message}: {sense}")),
        |reason| Error::Feed(format!("{reason}: {sense}")),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_sense_and_identifies_confirmed_feed_errors() {
        let mut data = [0_u8; 18];
        data[2] = 0x43;
        data[3..7].copy_from_slice(&42_u32.to_be_bytes());
        data[12] = 0x80;
        data[13] = 0x03;
        let sense = Sense::parse(&data).unwrap();
        assert!(sense.eom);
        assert!(sense.no_paper());
        assert_eq!(sense.info, 42);
        assert_eq!(sense.medium_error().unwrap(), "no paper");
        assert!(Sense::parse(&data[..13]).is_err());
    }
}
