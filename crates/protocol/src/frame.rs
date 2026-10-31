//! One frame on the wire (PROTOCOL.md §2).
//!
//! ```text
//! start  command  type  len  data (len bytes)  checksum
//! ```
//!
//! The checksum is `(type + len + sum(data)) mod 256`; it excludes the start
//! byte and the command.

use std::fmt;

use crate::register::Register;

/// Command bytes (PROTOCOL.md §3).
pub mod command {
    /// GET request from the host, data frame from the device.
    pub const DATA: u8 = 0xA1;
    /// SET from the host. The device never acknowledges it.
    pub const SET: u8 = 0xB1;
    /// Session open from the host.
    pub const CONNECT: u8 = 0xC1;
}

/// Which way a frame travels; the start byte tells.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    HostToDevice,
    DeviceToHost,
}

impl Dir {
    pub const fn start_byte(self) -> u8 {
        match self {
            Self::HostToDevice => 0xF1,
            Self::DeviceToHost => 0xF0,
        }
    }
}

/// Checksum over `type`, `len` and the data bytes.
pub fn checksum(id: u8, data: &[u8]) -> u8 {
    let len = data.len() as u8;
    data.iter()
        .fold(id.wrapping_add(len), |sum, byte| sum.wrapping_add(*byte))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameError {
    /// A frame carries at most 255 data bytes.
    DataTooLong(usize),
}

impl fmt::Display for FrameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DataTooLong(len) => write!(f, "{len} data bytes do not fit in a frame"),
        }
    }
}

impl std::error::Error for FrameError {}

/// A frame whose length and checksum are right by construction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Frame {
    dir: Dir,
    command: u8,
    id: u8,
    data: Vec<u8>,
}

impl Frame {
    pub fn new(
        dir: Dir,
        command: u8,
        id: u8,
        data: impl Into<Vec<u8>>,
    ) -> Result<Self, FrameError> {
        let data = data.into();
        if data.len() > usize::from(u8::MAX) {
            return Err(FrameError::DataTooLong(data.len()));
        }
        Ok(Self {
            dir,
            command,
            id,
            data,
        })
    }

    /// `F1 A1 <type> 01 00 <(type+1) mod 256>`.
    pub fn get(register: Register) -> Self {
        Self {
            dir: Dir::HostToDevice,
            command: command::DATA,
            id: register.id(),
            data: vec![0],
        }
    }

    /// `F1 C1 00 01 01 02`, sent on every open (PRT-01).
    pub fn connect() -> Self {
        Self {
            dir: Dir::HostToDevice,
            command: command::CONNECT,
            id: 0,
            data: vec![1],
        }
    }

    pub fn dir(&self) -> Dir {
        self.dir
    }

    pub fn command(&self) -> u8 {
        self.command
    }

    /// The `type` byte.
    pub fn id(&self) -> u8 {
        self.id
    }

    /// The register named by the `type` byte, if it is a known one.
    pub fn register(&self) -> Option<Register> {
        Register::from_id(self.id)
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(5 + self.data.len());
        bytes.extend([
            self.dir.start_byte(),
            self.command,
            self.id,
            self.data.len() as u8,
        ]);
        bytes.extend(&self.data);
        bytes.push(checksum(self.id, &self.data));
        bytes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prt_02_get_frame_matches_the_spec_formula() {
        for id in 0..=u8::MAX {
            let frame = Frame::new(Dir::HostToDevice, command::DATA, id, [0]).unwrap();
            assert_eq!(
                frame.encode(),
                [0xF1, 0xA1, id, 0x01, 0x00, id.wrapping_add(1)]
            );
        }
    }

    #[test]
    fn prt_01_connect_frame_is_the_documented_bytes() {
        assert_eq!(
            Frame::connect().encode(),
            [0xF1, 0xC1, 0x00, 0x01, 0x01, 0x02]
        );
    }

    #[test]
    fn prt_02_get_dump_frame() {
        assert_eq!(
            Frame::get(Register::Dump).encode(),
            [0xF1, 0xA1, 0xFF, 0x01, 0x00, 0x00]
        );
    }

    #[test]
    fn prt_02_checksum_wraps_and_covers_type_len_and_data() {
        // From the capture: F0 A1 C0 04 9B DF A1 41 <ck>.
        assert_eq!(checksum(0xC0, &[0x9B, 0xDF, 0xA1, 0x41]), 0x20); // 0x420 mod 256
        assert_eq!(checksum(0xFF, &[0xFF; 3]), 0xFF); // 0x4FF mod 256
    }

    #[test]
    fn prt_02_frame_carries_at_most_255_data_bytes() {
        assert!(Frame::new(Dir::DeviceToHost, command::DATA, 0xFF, vec![0; 255]).is_ok());
        assert_eq!(
            Frame::new(Dir::DeviceToHost, command::DATA, 0xFF, vec![0; 256]),
            Err(FrameError::DataTooLong(256))
        );
    }
}
