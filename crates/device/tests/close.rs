//! Closing the session (SAF-14) against `SimDevice`, level 1.

mod common;

use std::time::Duration;

use common::{Rig, secs};
use device::{Event, Refusal, State};
use protocol::value::Millivolts;
use protocol::{Frame, Write, command};

fn closes(rig: &Rig) -> Vec<(Duration, Frame)> {
    rig.written
        .iter()
        .filter(|(_, f)| f.command() == command::CONNECT && f.data() == [0])
        .cloned()
        .collect()
}

fn closed_events(rig: &Rig) -> usize {
    rig.events.iter().filter(|e| **e == Event::Closed).count()
}

#[test]
fn saf_14_the_close_frame_is_the_documented_bytes_and_closes_the_simulated_session() {
    let mut rig = Rig::online();
    assert!(rig.sim.is_locked(), "the connect frame locked the panel");
    rig.link.close();
    rig.run(secs(1));
    let sent = closes(&rig);
    assert_eq!(sent.len(), 1);
    assert_eq!(sent[0].1.encode(), [0xF1, 0xC1, 0x00, 0x01, 0x00, 0x01]);
    assert!(!rig.sim.is_locked(), "the panel is released");
    assert!(!rig.sim.is_connected(), "and the device is silent again");
    assert_eq!(rig.sim.dropped(), 0, "the device kept the close frame");
    assert_eq!(rig.link.state(), State::Closed);
    assert_eq!(closed_events(&rig), 1);
}

#[test]
fn saf_14_nothing_follows_the_close_frame() {
    let mut rig = Rig::online();
    rig.link.close();
    rig.run(secs(3));
    let (at, _) = closes(&rig)[0].clone();
    let after: Vec<_> = rig.written.iter().filter(|(t, _)| *t > at).collect();
    assert!(after.is_empty(), "frames after the close: {after:?}");
    assert_eq!(rig.link.state(), State::Closed);
}

#[test]
fn saf_14_polling_stops_when_the_close_starts() {
    let mut rig = Rig::online();
    rig.link.close();
    let before = rig.polls(Duration::ZERO);
    rig.run(secs(2));
    assert_eq!(
        rig.polls(Duration::ZERO),
        before,
        "no poll after the request"
    );
}

#[test]
fn saf_14_a_queued_write_goes_out_before_the_close_frame_one_gap_apart() {
    let mut rig = Rig::online();
    rig.link
        .write(Write::SetVoltage(Millivolts::new(7000)))
        .unwrap();
    rig.link.close();
    rig.run(secs(1));
    assert_eq!(rig.sim.set_voltage(), 7.0, "the last write was not lost");
    let sets: Vec<Duration> = rig
        .written
        .iter()
        .filter(|(_, f)| f.command() == command::SET)
        .map(|(t, _)| *t)
        .collect();
    let close_at = closes(&rig)[0].0;
    assert!(
        close_at - sets[0] >= Duration::from_millis(100),
        "one SET_GAP between"
    );
    assert_eq!(rig.sim.dropped(), 0);
}

#[test]
fn saf_14_the_off_of_a_quit_goes_first_and_the_close_follows_the_whole_queue() {
    let mut rig = Rig::online();
    rig.sim.panel_output(true);
    rig.run(secs(1));
    rig.link.write(Write::Output(false)).unwrap();
    rig.link.close();
    rig.run(secs(1));
    assert!(!rig.sim.is_output_on());
    let ids: Vec<(u8, u8)> = rig
        .written
        .iter()
        .filter(|(_, f)| f.command() != command::DATA)
        .map(|(_, f)| (f.command(), f.id()))
        .collect();
    assert_eq!(
        &ids[ids.len() - 2..],
        [(command::SET, 0xDB), (command::CONNECT, 0x00)],
        "OFF, then the close"
    );
}

#[test]
fn saf_14_after_closing_writes_are_refused_and_nothing_reconnects() {
    let mut rig = Rig::online();
    rig.link.close();
    rig.run(secs(1));
    assert_eq!(
        rig.link.write(Write::Output(false)),
        Err(Refusal::NotOnline)
    );
    rig.run(secs(5));
    assert_eq!(rig.link.state(), State::Closed);
    assert!(!rig.link.wants_open(rig.now));
    assert_eq!(closes(&rig).len(), 1);
}

#[test]
fn saf_14_closing_a_link_that_is_down_just_ends() {
    let mut rig = Rig::online();
    rig.sim.unplug();
    rig.run_until(secs(2), |r| r.link.state() == State::Lost);
    rig.events.clear();
    rig.link.close();
    rig.run(Duration::from_millis(50));
    assert_eq!(rig.link.state(), State::Closed);
    assert!(closes(&rig).is_empty(), "no port, no frame");
    assert_eq!(closed_events(&rig), 1);
}

#[test]
fn saf_14_closing_twice_is_the_same_as_once() {
    let mut rig = Rig::online();
    rig.link.close();
    rig.link.close();
    rig.run(secs(1));
    assert_eq!(closes(&rig).len(), 1);
    rig.link.close();
    rig.run(secs(1));
    assert_eq!(closes(&rig).len(), 1);
}
