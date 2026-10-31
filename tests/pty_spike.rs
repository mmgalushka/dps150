//! M0 spike: the level 2 tests need `serialport` to open a pty by path at the
//! device's baud rate, as `dps150 --port <path>` will (DEVOPS.md §4).
#![cfg(unix)]

use std::io::{Read, Write};
use std::time::Duration;

use serialport::{SerialPort, TTYPort};

const BAUD: u32 = 115_200;
const TIMEOUT: Duration = Duration::from_secs(2);

fn read_exact(port: &mut TTYPort, len: usize) -> Vec<u8> {
    let mut bytes = vec![0; len];
    port.read_exact(&mut bytes).expect("read from pty");
    bytes
}

#[test]
fn serialport_opens_pty_by_path() {
    let (mut master, slave) = TTYPort::pair().expect("create pty pair");
    master.set_timeout(TIMEOUT).expect("set master timeout");
    let path = slave.name().expect("pty slave has a path");
    drop(slave);

    let mut client = serialport::new(&path, BAUD)
        .timeout(TIMEOUT)
        .open_native()
        .expect("open pty by path");

    let connect = [0xF1, 0xC1, 0x00, 0x01, 0x01, 0x02];
    client.write_all(&connect).expect("write to pty");
    assert_eq!(read_exact(&mut master, connect.len()), connect);

    let reply = [0xF0, 0xA1, 0xC3, 0x00, 0xC3];
    master.write_all(&reply).expect("write to pty master");
    assert_eq!(read_exact(&mut client, reply.len()), reply);
}
