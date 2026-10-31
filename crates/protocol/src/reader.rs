//! Turns a byte stream into frames (PRT-02, APP-03, APP-04).
//!
//! The reader owns no clock: the caller passes the time with every call, so
//! the 200 ms rule tests without sleeping (PLAN.md §2).

use std::time::Duration;

use crate::frame::{Dir, Frame, checksum};
use crate::register::expected_len;

/// An incomplete frame is discarded after this long without a new byte
/// (APP-04).
pub const STALE_FRAME: Duration = Duration::from_millis(200);

/// What the reader threw away; the device layer exposes it to tests (APP-03).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Stats {
    pub bad_checksum: u64,
    pub bad_length: u64,
    /// Bytes skipped while looking for a start byte.
    pub skipped_bytes: u64,
    /// Incomplete frames dropped by the 200 ms rule.
    pub stale_frames: u64,
}

pub struct Reader {
    dir: Dir,
    buf: Vec<u8>,
    last_byte: Duration,
    stats: Stats,
}

impl Reader {
    /// A reader of frames travelling in direction `dir`.
    pub fn new(dir: Dir) -> Self {
        Self {
            dir,
            buf: Vec::new(),
            last_byte: Duration::ZERO,
            stats: Stats::default(),
        }
    }

    pub fn stats(&self) -> Stats {
        self.stats
    }

    /// Bytes held back as the start of an incomplete frame.
    pub fn pending(&self) -> usize {
        self.buf.len()
    }

    /// Takes `bytes` read at time `now` and returns the frames they complete.
    ///
    /// Frames with a bad checksum, or of a known kind with the wrong length,
    /// are dropped and scanning resumes at the byte after their start byte.
    /// Frames of an unknown type are returned; the caller ignores them.
    pub fn feed(&mut self, now: Duration, bytes: &[u8]) -> Vec<Frame> {
        if !self.buf.is_empty() && now.saturating_sub(self.last_byte) >= STALE_FRAME {
            self.buf.clear();
            self.stats.stale_frames += 1;
        }
        if bytes.is_empty() {
            return Vec::new();
        }
        self.buf.extend_from_slice(bytes);
        self.last_byte = now;

        let mut frames = Vec::new();
        loop {
            let start = self.dir.start_byte();
            let skip = self
                .buf
                .iter()
                .position(|byte| *byte == start)
                .unwrap_or(self.buf.len());
            self.stats.skipped_bytes += skip as u64;
            self.buf.drain(..skip);
            if self.buf.len() < 4 {
                break;
            }
            let (command, id, len) = (self.buf[1], self.buf[2], usize::from(self.buf[3]));
            if expected_len(self.dir, command, id).is_some_and(|expected| expected != len) {
                self.stats.bad_length += 1;
                self.buf.drain(..1);
                continue;
            }
            if self.buf.len() < 5 + len {
                break;
            }
            let data = &self.buf[4..4 + len];
            if checksum(id, data) != self.buf[4 + len] {
                self.stats.bad_checksum += 1;
                self.buf.drain(..1);
                continue;
            }
            let frame = Frame::new(self.dir, command, id, data).expect("len is a u8");
            self.buf.drain(..5 + len);
            frames.push(frame);
        }
        frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frame::command;

    const T0: Duration = Duration::ZERO;

    fn device(id: u8, data: &[u8]) -> Vec<u8> {
        Frame::new(Dir::DeviceToHost, command::DATA, id, data)
            .unwrap()
            .encode()
    }

    fn temperature() -> Vec<u8> {
        device(0xC4, &[0x9B, 0xDF, 0xA1, 0x41])
    }

    fn ids(frames: &[Frame]) -> Vec<u8> {
        frames.iter().map(Frame::id).collect()
    }

    fn reader() -> Reader {
        Reader::new(Dir::DeviceToHost)
    }

    #[test]
    fn prt_02_back_to_back_frames_all_come_out() {
        let mut stream = temperature();
        stream.extend(device(0xC0, &[0; 4]));
        stream.extend(device(0xC3, &[0; 12]));
        assert_eq!(ids(&reader().feed(T0, &stream)), [0xC4, 0xC0, 0xC3]);
    }

    #[test]
    fn prt_02_pushed_frames_interleave_with_a_reply() {
        // Replies are matched by type, not by arrival order.
        let mut stream = device(0xC0, &[0; 4]);
        stream.extend(device(0xFF, &[0; 139]));
        stream.extend(device(0xC4, &[0; 4]));
        let frames = reader().feed(T0, &stream);
        let reply = frames.iter().find(|f| f.id() == 0xFF).unwrap();
        assert_eq!(reply.data().len(), 139);
        assert_eq!(ids(&frames), [0xC0, 0xFF, 0xC4]);
    }

    #[test]
    fn prt_02_stream_opens_in_the_middle_of_a_frame() {
        let mut stream = vec![0x41, 0xA1, 0x07]; // tail of a frame sent before open
        stream.extend(temperature());
        let mut reader = reader();
        assert_eq!(ids(&reader.feed(T0, &stream)), [0xC4]);
        assert_eq!(reader.stats().skipped_bytes, 3);
    }

    #[test]
    fn app_03_partial_frame_is_kept_until_completed() {
        let stream = temperature();
        let mut reader = reader();
        for byte in &stream[..stream.len() - 1] {
            assert!(reader.feed(T0, &[*byte]).is_empty());
        }
        assert_eq!(reader.pending(), stream.len() - 1);
        assert_eq!(ids(&reader.feed(T0, &stream[stream.len() - 1..])), [0xC4]);
        assert_eq!(reader.pending(), 0);
    }

    #[test]
    fn app_03_bad_checksum_drops_the_frame_and_resumes_after_its_start_byte() {
        // A good frame hidden inside the data of a frame with a bad checksum.
        let inner = temperature();
        let mut data = inner.clone();
        data.extend([0; 3]);
        let mut outer = Frame::new(Dir::DeviceToHost, command::DATA, 0xC3, data)
            .unwrap()
            .encode();
        *outer.last_mut().unwrap() ^= 0xFF;
        let mut reader = reader();
        let frames = reader.feed(T0, &outer);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].encode(), inner);
        assert_eq!(reader.stats().bad_checksum, 1);
    }

    #[test]
    fn app_03_known_type_with_wrong_length_is_dropped_without_waiting_for_its_data() {
        let mut stream = vec![0xF0, 0xA1, 0xC0, 0x05]; // C0 is four bytes
        let mut reader = reader();
        assert!(reader.feed(T0, &stream).is_empty());
        assert_eq!(reader.pending(), 0);
        assert_eq!(reader.stats().bad_length, 1);
        stream = temperature();
        assert_eq!(ids(&reader.feed(T0, &stream)), [0xC4]);
    }

    #[test]
    fn app_03_unknown_type_comes_out_for_the_caller_to_ignore() {
        let frames = reader().feed(T0, &device(0x42, &[1, 2, 3]));
        assert_eq!(ids(&frames), [0x42]);
        assert_eq!(frames[0].register(), None);
    }

    #[test]
    fn app_03_data_bytes_equal_to_the_start_byte_do_not_split_a_frame() {
        let frame = device(0xC0, &[0xF0, 0xF0, 0xF0, 0xF0]);
        let frames = reader().feed(T0, &frame);
        assert_eq!(frames.len(), 1);
        assert_eq!(frames[0].data(), [0xF0; 4]);
    }

    #[test]
    fn app_04_incomplete_frame_is_discarded_after_200_ms_without_a_byte() {
        let stream = temperature();
        let mut reader = reader();
        reader.feed(T0, &stream[..5]);
        // The rest arrives 200 ms later: the head is gone, the tail is noise.
        let late = reader.feed(STALE_FRAME, &stream[5..]);
        assert!(late.is_empty());
        assert_eq!(reader.stats().stale_frames, 1);
        assert_eq!(reader.pending(), 0);
    }

    #[test]
    fn app_04_incomplete_frame_survives_199_ms() {
        let stream = temperature();
        let mut reader = reader();
        reader.feed(T0, &stream[..5]);
        let rest = reader.feed(STALE_FRAME - Duration::from_millis(1), &stream[5..]);
        assert_eq!(ids(&rest), [0xC4]);
        assert_eq!(reader.stats().stale_frames, 0);
    }

    #[test]
    fn app_04_every_byte_restarts_the_200_ms_window() {
        let stream = temperature();
        let mut reader = reader();
        let mut now = T0;
        let mut frames = Vec::new();
        for byte in &stream {
            frames.extend(reader.feed(now, &[*byte]));
            now += Duration::from_millis(150);
        }
        assert_eq!(ids(&frames), [0xC4]);
    }

    #[test]
    fn prt_02_host_reader_reads_get_set_and_connect() {
        let mut stream = Frame::connect().encode();
        stream.extend(Frame::get(crate::Register::Dump).encode());
        stream.extend(
            Frame::new(Dir::HostToDevice, command::SET, 0xC1, 3.3_f32.to_le_bytes())
                .unwrap()
                .encode(),
        );
        let frames = Reader::new(Dir::HostToDevice).feed(T0, &stream);
        assert_eq!(frames.len(), 3);
        assert_eq!(frames[2].data(), 3.3_f32.to_le_bytes());
    }

    #[test]
    fn app_03_host_set_with_wrong_length_is_dropped() {
        let bad = Frame::new(Dir::HostToDevice, command::SET, 0xC1, [1, 2])
            .unwrap()
            .encode();
        let mut reader = Reader::new(Dir::HostToDevice);
        assert!(reader.feed(T0, &bad).is_empty());
        assert_eq!(reader.stats().bad_length, 1);
    }
}
