//! M1 exit: the recorded device traffic decodes with no errors.

use std::path::PathBuf;
use std::time::Duration;

use protocol::{Dir, Frame, Reader, Reply, Stats};

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|error| panic!("{}: {error}", path.display()))
}

fn read_in_chunks(dir: Dir, bytes: &[u8], chunk: usize) -> (Vec<Frame>, Stats) {
    let mut reader = Reader::new(dir);
    let mut frames = Vec::new();
    for piece in bytes.chunks(chunk) {
        frames.extend(reader.feed(Duration::ZERO, piece));
    }
    assert_eq!(reader.pending(), 0, "capture ends on a frame boundary");
    (frames, reader.stats())
}

#[test]
fn prt_02_all_345_device_frames_decode_with_no_errors() {
    let bytes = fixture("capture-dev-to-host.bin");
    let (frames, stats) = read_in_chunks(Dir::DeviceToHost, &bytes, bytes.len());
    assert_eq!(frames.len(), 345);
    assert_eq!(stats, Stats::default());
    for frame in &frames {
        let reply = Reply::from_frame(frame).unwrap_or_else(|error| panic!("{frame:?}: {error}"));
        assert!(
            matches!(
                reply,
                Reply::InputVoltage(_)
                    | Reply::Output(_)
                    | Reply::Temperature(_)
                    | Reply::MaxVoltage(_)
                    | Reply::MaxCurrent(_)
            ),
            "{reply:?}"
        );
    }
}

#[test]
fn prt_02_capture_decodes_the_same_whatever_the_read_size() {
    let bytes = fixture("capture-dev-to-host.bin");
    let (whole, _) = read_in_chunks(Dir::DeviceToHost, &bytes, bytes.len());
    for chunk in [1, 2, 3, 5, 7, 13, 64, 139, 1000] {
        let (frames, stats) = read_in_chunks(Dir::DeviceToHost, &bytes, chunk);
        assert_eq!(frames, whole, "chunk size {chunk}");
        assert_eq!(stats, Stats::default(), "chunk size {chunk}");
    }
}

#[test]
fn prt_02_capture_matches_the_values_in_the_spec() {
    // PROTOCOL.md §5: first frames carry 20.234 V in, 23.608 °C, live max 20.034 V, 5.1 A.
    let bytes = fixture("capture-dev-to-host.bin");
    let (frames, _) = read_in_chunks(Dir::DeviceToHost, &bytes, bytes.len());
    let first = |wanted: fn(&Reply) -> bool| {
        frames
            .iter()
            .map(|f| Reply::from_frame(f).unwrap())
            .find(wanted)
            .unwrap()
    };
    let Reply::InputVoltage(volts) = first(|r| matches!(r, Reply::InputVoltage(_))) else {
        panic!()
    };
    assert!((volts - 20.234).abs() < 1e-3, "{volts}");
    let Reply::Temperature(celsius) = first(|r| matches!(r, Reply::Temperature(_))) else {
        panic!()
    };
    assert!((celsius - 23.608).abs() < 1e-3, "{celsius}");
    assert_eq!(
        first(|r| matches!(r, Reply::MaxVoltage(_))),
        Reply::MaxVoltage(protocol::Millivolts::new(20_034))
    );
    assert_eq!(
        first(|r| matches!(r, Reply::MaxCurrent(_))),
        Reply::MaxCurrent(protocol::Milliamps::new(5_100))
    );
}

#[test]
fn prt_02_host_traffic_in_the_capture_decodes() {
    let bytes = fixture("capture-host-to-dev.bin");
    let (frames, stats) = read_in_chunks(Dir::HostToDevice, &bytes, bytes.len());
    assert_eq!(stats, Stats::default());
    assert_eq!(frames.len(), 7);
    assert_eq!(frames[0], Frame::connect());
    let encoded: Vec<u8> = frames.iter().flat_map(Frame::encode).collect();
    assert_eq!(encoded, bytes, "re-encoding gives the recorded bytes");
}
