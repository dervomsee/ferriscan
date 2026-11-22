use std::collections::HashMap;
use std::io::Write;
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use ferriscan::adapters::ix500::scanner::{Scanner, mode_block, release, window_block};
use ferriscan::adapters::ix500::scsi::ScsiChannel;
use ferriscan::adapters::scansnap_wifi::wire::{self, VENS};
use ferriscan::core::{Error, MultifeedDetection, PageSize, ScanSettings, Sheet};

const MAC: [u8; 6] = [2, 0, 0, 0, 0, 1];
// Raw image-info block captured from a live iX500 at 300 dpi (docs/protocol-ix500.md, "Image info").
const INFO: &str = "00000a300000113000000a3000000e500000012c012c00000a3400000e550000";

#[derive(Debug, Default)]
struct Recording {
    commands: Vec<Vec<u8>>,
    blocks: HashMap<u8, Vec<u8>>,
    out_lengths: HashMap<u8, u32>,
}

struct MockIx500 {
    port: u16,
    images: [Vec<u8>; 2],
    server: JoinHandle<Recording>,
}

impl MockIx500 {
    fn start(sheets: usize, jam_at: Option<usize>, chunk_size: Option<usize>) -> Self {
        let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let images = [
            vec![0xff, 0xd8, 1, 2, 3, 4, 0xff, 0xd9],
            vec![0xff, 0xd8, 8, 7, 6, 5, 0xff, 0xd9],
        ];
        let server_images = images.clone();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .write_all(&wire::server_frame(VENS, 0, &[]).unwrap())
                .unwrap();
            let mut recording = Recording::default();
            let mut fed = 0_usize;
            let mut positions = HashMap::<u8, usize>::new();
            let mut current_sense = sense(0, 0, 0, false);
            while let Ok(request) = wire::read_frame(&mut stream, VENS) {
                let cdb_len = u32::from_be_bytes(request[32..36].try_into().unwrap()) as usize;
                let cdb = request[48..48 + cdb_len].to_vec();
                let out = request[64..].to_vec();
                recording.commands.push(cdb.clone());
                if !out.is_empty() {
                    recording.blocks.insert(cdb[0], out);
                    recording.out_lengths.insert(
                        cdb[0],
                        u32::from_be_bytes(request[40..44].try_into().unwrap()),
                    );
                }
                let (status, data) = handle_command(
                    &cdb,
                    sheets,
                    jam_at,
                    chunk_size,
                    &server_images,
                    &mut fed,
                    &mut positions,
                    &mut current_sense,
                );
                let mut body = vec![0; 24];
                body.extend_from_slice(&data);
                stream
                    .write_all(&wire::server_frame(VENS, status, &body).unwrap())
                    .unwrap();
            }
            recording
        });
        Self {
            port,
            images,
            server,
        }
    }

    fn channel(&self) -> ScsiChannel {
        ScsiChannel::connect(Ipv4Addr::LOCALHOST, MAC, self.port, Duration::from_secs(2)).unwrap()
    }

    fn finish(self) -> Recording {
        self.server.join().unwrap()
    }
}

#[allow(clippy::too_many_arguments)]
fn handle_command(
    cdb: &[u8],
    sheets: usize,
    jam_at: Option<usize>,
    chunk_size: Option<usize>,
    images: &[Vec<u8>; 2],
    fed: &mut usize,
    positions: &mut HashMap<u8, usize>,
    current_sense: &mut [u8; 18],
) -> (u32, Vec<u8>) {
    match cdb[0] {
        0x03 => {
            let response = current_sense.to_vec();
            *current_sense = sense(0, 0, 0, false);
            (0, response)
        }
        0xc2 => (0, vec![0; 0x20]),
        0xd5 => (0, vec![0; 8]),
        0xe0 => {
            *fed += 1;
            if Some(*fed) == jam_at {
                *current_sense = sense(3, 0x80, 0x01, false);
                (2, Vec::new())
            } else if *fed > sheets {
                *current_sense = sense(3, 0x80, 0x03, false);
                (2, Vec::new())
            } else {
                (0, Vec::new())
            }
        }
        0x28 if cdb[2] == 0x80 => (0, hex::decode(INFO).unwrap()),
        0x28 => {
            let index = cdb[10];
            let image = &images[usize::from(index % 2)];
            let position = positions.get(&index).copied().unwrap_or(0);
            let size = chunk_size.unwrap_or(image.len());
            let end = (position + size).min(image.len());
            let chunk = u8::try_from(position / size).unwrap();
            let mut data = vec![index, chunk];
            data.extend_from_slice(&image[position..end]);
            positions.insert(index, end);
            if end < image.len() {
                (0, data)
            } else {
                *current_sense = sense(0, 0, 0, true);
                (2, data)
            }
        }
        _ => (0, Vec::new()),
    }
}

fn sense(key: u8, asc: u8, ascq: u8, eom: bool) -> [u8; 18] {
    let mut data = [0_u8; 18];
    data[0] = 0xf0;
    data[2] = key | if eom { 0x60 } else { 0 };
    data[7] = 0x0a;
    data[12] = asc;
    data[13] = ascq;
    data
}

#[test]
fn two_sheets_follow_the_documented_scan_sequence() {
    let mock = MockIx500::start(2, None, None);
    let expected_images = mock.images.clone();
    let mut pages = Vec::new();
    {
        let mut channel = mock.channel();
        Scanner::new(&mut channel)
            .scan(&ScanSettings::default(), &mut |sheet| {
                pages.push(sheet);
                Ok(())
            })
            .unwrap();
    }
    let recording = mock.finish();

    assert_eq!(pages.len(), 2);
    assert_eq!(pages[0].front, expected_images[0]);
    assert_eq!(pages[0].back.as_ref().unwrap(), &expected_images[1]);
    let info = pages[0].info.as_ref().unwrap();
    assert_eq!((info.width, info.height), (2608, 3664));
    assert_eq!((info.dpi_x, info.dpi_y), (300, 300));
    let ops = recording
        .commands
        .iter()
        .map(|cdb| cdb[0])
        .collect::<Vec<_>>();
    assert_eq!(&ops[..7], &[0xc2, 0xd5, 0xd8, 0xe9, 0xd4, 0xc2, 0x03]);
    let per_sheet = [0xe0, 0x28, 0x03, 0x28, 0x03, 0x28, 0xc2];
    assert_eq!(&ops[7..21], &per_sheet.repeat(2));
    assert_eq!(&ops[21..], &[0xe0, 0x03]);

    let reads = recording
        .commands
        .iter()
        .filter(|cdb| cdb[0] == 0x28)
        .map(|cdb| (cdb[2], cdb[5], cdb[10]))
        .collect::<Vec<_>>();
    assert_eq!(
        reads,
        [
            (0, 0, 0),
            (0, 0x80, 1),
            (0x80, 0x80, 1),
            (0, 0, 2),
            (0, 0x80, 3),
            (0x80, 0x80, 3),
        ]
    );
    assert_eq!(recording.out_lengths[&0xd5], 8);
    assert_eq!(recording.out_lengths[&0xe9], 32);
    assert_eq!(recording.out_lengths[&0xd4], 80);
    assert_eq!(
        recording.blocks[&0xe9],
        window_block(300, PageSize::Auto).unwrap().to_vec()
    );
    assert_eq!(
        recording.blocks[&0xd4],
        mode_block(300, PageSize::Auto, MultifeedDetection::Ultrasonic)
            .unwrap()
            .to_vec()
    );
}

#[test]
fn empty_feeder_has_a_specific_error() {
    let mock = MockIx500::start(0, None, None);
    let error = {
        let mut channel = mock.channel();
        Scanner::new(&mut channel)
            .scan(&ScanSettings::default(), &mut |_| Ok(()))
            .unwrap_err()
    };
    mock.finish();
    assert!(matches!(error, Error::NoPaper));
}

#[test]
fn jam_after_one_sheet_preserves_delivered_page_and_reports_feed_error() {
    let mock = MockIx500::start(3, Some(2), None);
    let mut pages = Vec::<Sheet>::new();
    let error = {
        let mut channel = mock.channel();
        Scanner::new(&mut channel)
            .scan(&ScanSettings::default(), &mut |sheet| {
                pages.push(sheet);
                Ok(())
            })
            .unwrap_err()
    };
    mock.finish();
    assert_eq!(pages.len(), 1);
    assert!(matches!(error, Error::Feed(message) if message.contains("paper jam")));
}

#[test]
fn chunked_images_are_reassembled_and_simplex_discards_back() {
    let mock = MockIx500::start(1, None, Some(3));
    let expected_front = mock.images[0].clone();
    let mut pages = Vec::new();
    let settings = ScanSettings {
        duplex: false,
        ..ScanSettings::default()
    };
    {
        let mut channel = mock.channel();
        Scanner::new(&mut channel)
            .scan(&settings, &mut |sheet| {
                pages.push(sheet);
                Ok(())
            })
            .unwrap();
    }
    let recording = mock.finish();
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].front, expected_front);
    assert!(pages[0].back.is_none());
    assert!(
        recording
            .commands
            .iter()
            .filter(|cdb| cdb[0] == 0x28 && cdb[2] == 0)
            .count()
            > 2
    );
}

#[test]
fn callback_errors_stop_scanning() {
    let mock = MockIx500::start(1, None, None);
    let error = {
        let mut channel = mock.channel();
        Scanner::new(&mut channel)
            .scan(&ScanSettings::default(), &mut |_| {
                Err(Error::Input("output failed".into()))
            })
            .unwrap_err()
    };
    mock.finish();
    assert!(matches!(error, Error::Input(message) if message == "output failed"));
}

#[test]
fn release_sends_d6_on_its_own_connection() {
    let mock = MockIx500::start(0, None, None);
    release(Ipv4Addr::LOCALHOST, MAC, mock.port).unwrap();
    let recording = mock.finish();
    assert_eq!(recording.commands, vec![vec![0xd6, 0, 0, 0, 0, 0]]);
}
