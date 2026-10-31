//! M0 spike: the level 2 tests need `serialport` to open a pty by path, as
//! `dps150 --port <path>` will (DEVOPS.md §4).
//!
//! Linux opens a pty at the device's baud rate. macOS does not: `serialport`
//! sets the rate there with the IOSSIOSPEED ioctl, which a pty refuses with
//! ENOTTY. Baud 0 skips the ioctl. If the macOS test fails because 115200
//! opened, level 2 can run on macOS.
#![cfg(unix)]

use std::io::{Read, Write};
use std::time::Duration;

use serialport::{SerialPort, TTYPort};

const TIMEOUT: Duration = Duration::from_secs(2);

fn pty() -> (TTYPort, String) {
    let (mut master, slave) = TTYPort::pair().expect("create pty pair");
    master.set_timeout(TIMEOUT).expect("set master timeout");
    let path = slave.name().expect("pty slave has a path");
    drop(slave);
    (master, path)
}

fn read_exact(port: &mut TTYPort, len: usize) -> Vec<u8> {
    let mut bytes = vec![0; len];
    port.read_exact(&mut bytes).expect("read from pty");
    bytes
}

fn open_and_exchange_frames(baud: u32) {
    let (mut master, path) = pty();
    let mut client = serialport::new(&path, baud)
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

#[cfg(target_os = "linux")]
#[test]
fn serialport_opens_pty_at_device_baud() {
    open_and_exchange_frames(115_200);
}

#[cfg(target_os = "macos")]
#[test]
fn serialport_refuses_pty_at_device_baud_on_macos() {
    let (_master, path) = pty();
    let error = serialport::new(&path, 115_200)
        .open_native()
        .expect_err("115200 on a macOS pty");
    assert!(error.description.contains("Not a typewriter"), "{error:?}");
}

#[cfg(target_os = "macos")]
#[test]
fn serialport_opens_pty_at_baud_zero_on_macos() {
    open_and_exchange_frames(0);
}
