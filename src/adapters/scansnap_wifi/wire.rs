//! TCP framing shared by ScanSnap Wi-Fi protocol dialects.

use std::io::{self, Read};

use crate::core::{Error, Result};

pub const VENS: [u8; 4] = *b"VENS";
pub const SSNR: [u8; 4] = *b"ssNR";
pub const CLIENT_HEADER_LEN: usize = 32;
pub const SERVER_HEADER_LEN: usize = 16;
pub const MAX_FRAME: usize = 0x40_0000;

pub fn client_frame(
    magic: [u8; 4],
    command: u32,
    mac: [u8; 6],
    body: &[u8],
    extra: [u8; 4],
) -> Result<Vec<u8>> {
    let length = CLIENT_HEADER_LEN
        .checked_add(body.len())
        .ok_or_else(|| Error::Input("frame body is too large".into()))?;
    if length > MAX_FRAME {
        return Err(Error::Input(format!(
            "frame length {length} exceeds the {MAX_FRAME}-byte limit"
        )));
    }
    let length = u32::try_from(length)
        .map_err(|_| Error::Input("frame length does not fit in u32".into()))?;

    let mut frame = Vec::with_capacity(length as usize);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(&magic);
    frame.extend_from_slice(&command.to_be_bytes());
    frame.extend_from_slice(&0_u32.to_be_bytes());
    frame.extend_from_slice(&mac);
    frame.extend_from_slice(&0_u16.to_be_bytes());
    frame.extend_from_slice(&extra);
    frame.extend_from_slice(&0_u32.to_be_bytes());
    frame.extend_from_slice(body);
    Ok(frame)
}

/// Construct a server frame for protocol simulators and golden tests.
pub fn server_frame(magic: [u8; 4], status: u32, body: &[u8]) -> Result<Vec<u8>> {
    let length = SERVER_HEADER_LEN
        .checked_add(body.len())
        .ok_or_else(|| Error::Input("frame body is too large".into()))?;
    let length = u32::try_from(length)
        .map_err(|_| Error::Input("frame length does not fit in u32".into()))?;
    let mut frame = Vec::with_capacity(length as usize);
    frame.extend_from_slice(&length.to_be_bytes());
    frame.extend_from_slice(&magic);
    frame.extend_from_slice(&0_u32.to_be_bytes());
    frame.extend_from_slice(&status.to_be_bytes());
    frame.extend_from_slice(body);
    Ok(frame)
}

pub fn frame_status(frame: &[u8]) -> Result<u32> {
    if frame.len() < SERVER_HEADER_LEN {
        return Err(Error::Protocol(format!(
            "short server header: {} bytes",
            frame.len()
        )));
    }
    Ok(u32::from_be_bytes(frame[12..16].try_into().map_err(
        |_| Error::Protocol("invalid status field".into()),
    )?))
}

pub fn read_frame<R: Read>(reader: &mut R, magic: [u8; 4]) -> Result<Vec<u8>> {
    let mut head = [0_u8; 8];
    read_exact_counted(reader, &mut head)?;
    let length = u32::from_be_bytes(
        head[..4]
            .try_into()
            .map_err(|_| Error::Protocol("invalid frame length field".into()))?,
    ) as usize;
    if head[4..8] != magic {
        return Err(Error::Protocol(format!(
            "unexpected frame magic {:02x?}",
            &head[4..8]
        )));
    }
    if !(SERVER_HEADER_LEN..=MAX_FRAME).contains(&length) {
        return Err(Error::Protocol(format!("invalid frame length {length}")));
    }

    let mut frame = Vec::with_capacity(length);
    frame.extend_from_slice(&head);
    frame.resize(length, 0);
    read_exact_counted(reader, &mut frame[8..])?;
    Ok(frame)
}

fn read_exact_counted<R: Read>(reader: &mut R, mut buffer: &mut [u8]) -> Result<()> {
    let expected = buffer.len();
    let mut received = 0;
    while !buffer.is_empty() {
        match reader.read(buffer) {
            Ok(0) => {
                return Err(Error::Protocol(format!(
                    "connection closed after {received}/{expected} bytes"
                )));
            }
            Ok(count) => {
                received += count;
                buffer = &mut buffer[count..];
            }
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read};

    use super::*;

    const MAC: [u8; 6] = [2, 0, 0, 0, 0, 1];

    #[test]
    fn client_header_matches_ix500_layout() {
        let frame = client_frame(VENS, 0x13, MAC, &[], [0; 4]).unwrap();
        assert_eq!(frame.len(), CLIENT_HEADER_LEN);
        assert_eq!(
            &frame[..12],
            &[0, 0, 0, 32, b'V', b'E', b'N', b'S', 0, 0, 0, 0x13]
        );
        assert_eq!(&frame[16..22], &MAC);
        assert_eq!(&frame[22..], &[0; 10]);
    }

    #[test]
    fn client_body_and_extra_are_encoded_at_documented_offsets() {
        let frame = client_frame(VENS, 1, MAC, &[0xaa, 0xbb], [192, 0, 2, 10]).unwrap();
        assert_eq!(&frame[..4], &34_u32.to_be_bytes());
        assert_eq!(&frame[24..28], &[192, 0, 2, 10]);
        assert_eq!(&frame[32..], &[0xaa, 0xbb]);
    }

    struct OneByteAtATime(Cursor<Vec<u8>>);

    impl Read for OneByteAtATime {
        fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
            let mut one = [0_u8; 1];
            let count = self.0.read(&mut one)?;
            if count == 1 && !buffer.is_empty() {
                buffer[0] = one[0];
                Ok(1)
            } else {
                Ok(0)
            }
        }
    }

    #[test]
    fn reads_frames_from_short_socket_reads() {
        let frame = server_frame(VENS, 2, &[1, 2, 3]).unwrap();
        let mut reader = OneByteAtATime(Cursor::new(frame.clone()));
        assert_eq!(read_frame(&mut reader, VENS).unwrap(), frame);
        assert_eq!(frame_status(&frame).unwrap(), 2);
    }

    #[test]
    fn rejects_bad_magic_and_unbounded_lengths() {
        let frame = server_frame(SSNR, 0, &[]).unwrap();
        let error = read_frame(&mut Cursor::new(frame), VENS).unwrap_err();
        assert!(error.to_string().contains("magic"));

        let mut header = Vec::from((u32::try_from(MAX_FRAME).unwrap() + 1).to_be_bytes());
        header.extend_from_slice(&VENS);
        let error = read_frame(&mut Cursor::new(header), VENS).unwrap_err();
        assert!(error.to_string().contains("frame length"));
    }

    #[test]
    fn reports_truncated_frames_with_byte_counts() {
        let frame = server_frame(VENS, 0, &[1, 2, 3]).unwrap();
        let error = read_frame(&mut Cursor::new(frame[..18].to_vec()), VENS).unwrap_err();
        assert_eq!(
            error.to_string(),
            "protocol error: connection closed after 10/11 bytes"
        );
    }
}
