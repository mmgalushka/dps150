//! SIM-12: the simulator reproduces the captured session byte for byte.

use std::time::Duration;

use protocol::{Dir, Frame, Reader};
use simulator::SimDevice;

const HOST: &[u8] = include_bytes!("../../../tests/fixtures/capture-host-to-dev.bin");
const DEVICE: &[u8] = include_bytes!("../../../tests/fixtures/capture-dev-to-host.bin");

fn frames(dir: Dir, bytes: &[u8]) -> Vec<Frame> {
    Reader::new(dir).feed(Duration::ZERO, bytes)
}

fn raw(bytes: [u8; 4]) -> f32 {
    f32::from_le_bytes(bytes)
}

#[test]
fn sim_12_matches_the_first_five_push_frames_of_the_capture() {
    let mut sim = SimDevice::new();
    // The captured raw values, set explicitly so no arithmetic can differ.
    sim.set_input_voltage(raw([0x9B, 0xDF, 0xA1, 0x41]));
    sim.set_temperature(raw([0xA2, 0xDC, 0xBC, 0x41]));
    sim.set_live_max(Some(raw([0x01, 0x46, 0xA0, 0x41])));
    sim.set_max_current(raw([0x33, 0x33, 0xA3, 0x40]));

    let host = frames(Dir::HostToDevice, HOST);
    assert_eq!(host.len(), 7, "the connect frame and six GET C0");

    let mut replies = Vec::new();
    let mut pushes = Vec::new();
    for frame in &host {
        sim.feed(&frame.encode());
        replies.extend(frames(Dir::DeviceToHost, &sim.take_output()));
        sim.advance(Duration::from_millis(10));
        pushes.extend(frames(Dir::DeviceToHost, &sim.take_output()));
    }
    sim.advance(Duration::from_secs(2));
    pushes.extend(frames(Dir::DeviceToHost, &sim.take_output()));

    let captured = frames(Dir::DeviceToHost, DEVICE);
    assert_eq!(captured.len(), 345);
    let ours: Vec<Vec<u8>> = pushes.iter().take(5).map(Frame::encode).collect();
    let theirs: Vec<Vec<u8>> = captured.iter().take(5).map(Frame::encode).collect();
    assert_eq!(ours, theirs);

    assert_eq!(replies.len(), 6, "one C0 reply per GET");
    for reply in &replies {
        assert_eq!(reply.encode(), captured[1].encode(), "C0 as captured");
    }
}
