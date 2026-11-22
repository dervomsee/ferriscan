use std::io::Write;
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener};
use std::sync::mpsc::{self, Receiver};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use ferriscan::adapters::ix500::control::{Control, HostIdentity};
use ferriscan::adapters::scansnap_wifi::wire::{self, VENS};
use time::{Date, Month, PrimitiveDateTime, Time, UtcOffset};

const MAC: [u8; 6] = [2, 0, 0, 0, 0, 1];
const HOST_ID: [u8; 8] = [1, 2, 3, 4, 5, 6, 7, 8];

fn mock_control(replies: Vec<Vec<u8>>) -> (u16, Receiver<Vec<u8>>, JoinHandle<()>) {
    let listener = TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let (requests_tx, requests_rx) = mpsc::channel();
    let handle = thread::spawn(move || {
        for reply in replies {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .unwrap();
            stream
                .write_all(&wire::server_frame(VENS, 0, &[]).unwrap())
                .unwrap();
            let request = wire::read_frame(&mut stream, VENS).unwrap();
            requests_tx.send(request).unwrap();
            stream.write_all(&reply).unwrap();
        }
    });
    (port, requests_rx, handle)
}

fn registration_time() -> time::OffsetDateTime {
    PrimitiveDateTime::new(
        Date::from_calendar_date(2026, Month::October, 30).unwrap(),
        Time::from_hms(19, 6, 33).unwrap(),
    )
    .assume_offset(UtcOffset::UTC)
}

#[test]
fn registration_uses_greeting_and_one_control_connection() {
    let reply = wire::server_frame(VENS, 0, &[0; 4]).unwrap();
    let (port, requests, server) = mock_control(vec![reply]);
    let control = Control::new(Ipv4Addr::LOCALHOST, HostIdentity::new(HOST_ID, MAC))
        .with_port(port)
        .with_timeout(Duration::from_secs(2));

    control
        .register_at(Ipv4Addr::LOCALHOST, 6, Some("2345"), registration_time())
        .unwrap();

    let request = requests.recv().unwrap();
    server.join().unwrap();
    assert_eq!(request.len(), 128);
    assert_eq!(&request[8..12], &0x11_u32.to_be_bytes());
    assert_eq!(&request[16..22], &MAC);
    assert_eq!(request[32..].len(), 96);
    assert_eq!(&request[32..44], &[0, 6, 0x1e, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
    assert_eq!(&request[52..64], b"173132180179");
}

#[test]
fn name_and_status_use_separate_connections() {
    let mut name_body = [0_u8; 64];
    name_body[..16].copy_from_slice(b"iX500-A0XX012345");
    let name_reply = wire::server_frame(VENS, 0, &name_body).unwrap();
    let status_reply = wire::server_frame(VENS, 0, &3_u32.to_be_bytes()).unwrap();
    let (port, requests, server) = mock_control(vec![name_reply, status_reply]);
    let control = Control::new(Ipv4Addr::LOCALHOST, HostIdentity::new(HOST_ID, MAC))
        .with_port(port)
        .with_timeout(Duration::from_secs(2));

    assert_eq!(control.device_name().unwrap(), "iX500-A0XX012345");
    assert_eq!(control.wifi_status().unwrap(), 3);

    assert_eq!(&requests.recv().unwrap()[8..12], &0x13_u32.to_be_bytes());
    assert_eq!(&requests.recv().unwrap()[8..12], &0x30_u32.to_be_bytes());
    server.join().unwrap();
}
