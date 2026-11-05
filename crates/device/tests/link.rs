mod common;

use std::time::Duration;

use common::{MS, Rig, secs};
use device::constants::{HANDSHAKE_RESEND, RECONNECT_INTERVAL};
use device::{Cause, Event, Link, LinkConfig, OpenError, State};
use protocol::{Frame, Register, command};

// ---- handshake -------------------------------------------------------------

#[test]
fn prt_01_the_connect_frame_is_the_first_write_and_reads_wait_one_gap() {
    let mut rig = Rig::new();
    rig.run(Duration::from_millis(500));
    let (first_at, first) = &rig.written[0];
    assert_eq!(first.encode(), Frame::connect().encode());
    assert_eq!(*first_at, MS, "connect goes out at once");
    let (gets_at, get) = &rig.written[1];
    assert_eq!(get.id(), Register::Model.id());
    assert!(
        *gets_at - *first_at >= Duration::from_millis(100),
        "SET_GAP"
    );
    let ids: Vec<u8> = rig.written[1..5].iter().map(|(_, f)| f.id()).collect();
    assert_eq!(ids, [0xDE, 0xDF, 0xE0, 0xFF]);
    // The unit ignores GETs that arrive back to back.
    for pair in rig.written[1..5].windows(2) {
        assert!(
            pair[1].0 - pair[0].0 >= Duration::from_millis(100),
            "one gap between GETs"
        );
    }
}

#[test]
fn app_01_handshake_ends_online_with_names_and_a_full_dump() {
    let mut rig = Rig::new();
    assert_eq!(rig.link.state(), State::Opening);
    assert!(rig.link.snapshot().is_none());
    rig.run_until(secs(2), |rig| rig.link.state() == State::Online);
    assert_eq!(rig.events, [Event::Online { restored: false }]);
    let snapshot = rig.link.snapshot().expect("a dump");
    assert_eq!(snapshot.model.as_deref(), Some("DPS-150"));
    assert_eq!(snapshot.hardware.as_deref(), Some("V1.0"));
    assert_eq!(snapshot.firmware.as_deref(), Some("V1.2"));
    assert_eq!(snapshot.dump.set_voltage.get(), 3300);
    assert!(rig.link.writes_allowed());
}

#[test]
fn app_01_no_answer_resends_connect_once_and_fails_at_three_seconds() {
    let mut rig = Rig::new();
    rig.sim.unplug();
    rig.run(secs(4));
    let connects: Vec<Duration> = rig
        .written
        .iter()
        .filter(|(_, f)| f.command() == command::CONNECT)
        .map(|(at, _)| *at)
        .collect();
    assert_eq!(connects.len(), 2, "sent once, then once more");
    assert!(connects[1] >= HANDSHAKE_RESEND && connects[1] < HANDSHAKE_RESEND + 10 * MS);
    assert_eq!(rig.link.state(), State::Failed);
    assert_eq!(rig.events, [Event::Failed(Cause::NoAnswer)]);
    assert!(!rig.link.writes_allowed());
}

#[test]
fn app_01_a_port_that_cannot_be_opened_fails_the_start() {
    let mut link = Link::new(LinkConfig::default());
    let error = OpenError {
        reason: "No such file".into(),
        permission_denied: false,
    };
    link.open_failed(MS, error.clone());
    assert_eq!(link.state(), State::Failed);
    assert_eq!(link.take_events(), [Event::Failed(Cause::Open(error))]);
}

#[test]
fn app_01_split_replies_in_the_handshake_still_reach_online() {
    let mut rig = Rig::new();
    rig.sim.split(1);
    rig.run_until(secs(2), |rig| rig.link.state() == State::Online);
    assert_eq!(
        rig.link.snapshot().unwrap().model.as_deref(),
        Some("DPS-150")
    );
}

// ---- polling ---------------------------------------------------------------

#[test]
fn prt_06_polls_the_dump_at_the_refresh_rate() {
    for (hz, expected) in [(10, 10), (5, 5), (2, 2)] {
        let mut rig = Rig::with(LinkConfig {
            refresh_hz: hz,
            reconnect: true,
        });
        rig.run_until(secs(2), |rig| rig.link.state() == State::Online);
        let from = rig.now;
        rig.run(secs(1));
        assert_eq!(rig.polls(from), expected, "{hz} Hz");
    }
}

#[test]
fn prt_06_pushed_frames_update_the_model_without_answering_a_poll() {
    let mut rig = Rig::online();
    rig.sim.set_temperature(31.5);
    rig.sim.set_input_voltage(18.0);
    // A slow device: polls get no answer, but pushes keep arriving.
    rig.sim.delay(Duration::from_millis(250));
    let polls_before = rig.polls(Duration::ZERO);
    rig.run(Duration::from_millis(900));
    let dump = rig.link.snapshot().unwrap().dump;
    assert_eq!(dump.temperature, 31.5);
    assert_eq!(dump.input_voltage, 18.0);
    assert_eq!(dump.max_voltage.get(), 17800);
    assert!(
        rig.link.stats().missed_ticks > 0,
        "pushes did not clear the miss"
    );
    assert!(rig.polls(Duration::ZERO) > polls_before);
}

#[test]
fn prt_06_a_tick_with_a_reply_outstanding_is_skipped_not_sent() {
    let mut rig = Rig::online();
    rig.sim.delay(Duration::from_millis(150));
    let from = rig.now;
    rig.run(secs(1));
    // Each poll waits 150 ms: one tick in three is a send.
    assert!(rig.polls(from) < 10);
    assert!(rig.polls(from) >= 3);
    assert_eq!(
        rig.link.state(),
        State::Online,
        "one miss at a time is fine"
    );
}

#[test]
fn lnk_01_the_slowest_reply_seen_on_the_unit_does_not_lose_the_link() {
    let mut rig = Rig::online();
    rig.sim.delay(Duration::from_millis(125));
    rig.run(secs(5));
    assert_eq!(rig.link.state(), State::Online);
    assert!(rig.link.stats().missed_ticks > 0);
}

#[test]
fn lnk_01_three_missed_ticks_in_a_row_lose_the_link() {
    let mut rig = Rig::online();
    rig.events.clear();
    rig.sim.unplug();
    let silent_since = rig.now;
    rig.run_until(secs(2), |rig| rig.link.state() == State::Lost);
    let took = rig.now - silent_since;
    assert!(
        took >= Duration::from_millis(300) && took <= Duration::from_millis(420),
        "{took:?}"
    );
    assert_eq!(rig.events, [Event::Lost(Cause::MissedPolls)]);
    assert!(!rig.link.writes_allowed());
}

#[test]
fn lnk_01_a_reply_between_misses_resets_the_count() {
    let mut rig = Rig::online();
    for _ in 0..4 {
        rig.sim.delay(Duration::from_millis(250));
        rig.run(Duration::from_millis(500));
        rig.sim.delay(Duration::ZERO);
        rig.run(Duration::from_millis(500));
    }
    assert_eq!(rig.link.state(), State::Online);
}

#[test]
fn lnk_01_an_io_error_loses_the_link_at_once() {
    let mut rig = Rig::online();
    rig.events.clear();
    rig.link.io_error(rig.now, "Input/output error");
    assert_eq!(rig.link.state(), State::Lost);
    assert_eq!(
        rig.link.take_events(),
        [Event::Lost(Cause::Io("Input/output error".into()))]
    );
}

#[test]
fn lnk_02_the_last_snapshot_is_kept_after_loss_for_the_ui_to_grey_out() {
    let mut rig = Rig::online();
    rig.sim.unplug();
    rig.run_until(secs(2), |rig| rig.link.state() == State::Lost);
    assert!(
        rig.link.snapshot().is_some(),
        "stale, but state() says Lost"
    );
}

#[test]
fn app_03_corrupted_frames_are_dropped_and_counted_never_applied() {
    let mut rig = Rig::online();
    rig.sim.corrupt(1000);
    rig.run(secs(1));
    assert!(rig.link.stats().reader.bad_checksum > 0);
    assert_eq!(
        rig.events.last(),
        Some(&Event::Lost(Cause::MissedPolls)),
        "no valid reply: polls go missing"
    );
    assert_eq!(rig.link.snapshot().unwrap().dump.set_voltage.get(), 3300);
}

// ---- reconnect -------------------------------------------------------------

#[test]
fn lnk_06_with_reconnect_on_the_link_retries_every_second_and_restores() {
    let mut rig = Rig::online();
    rig.sim.unplug();
    rig.run_until(secs(2), |rig| rig.link.state() == State::Lost);
    let lost_at = rig.now;
    rig.events.clear();
    rig.sim.plug();
    rig.run_until(secs(3), |rig| rig.link.state() == State::Online);
    assert!(rig.now - lost_at >= RECONNECT_INTERVAL, "waited a second");
    assert_eq!(rig.events, [Event::Online { restored: true }]);
    assert!(rig.link.writes_allowed());
}

#[test]
fn lnk_06_with_reconnect_off_it_waits_for_the_request() {
    let mut rig = Rig::with(LinkConfig {
        refresh_hz: 10,
        reconnect: false,
    });
    rig.run_until(secs(2), |rig| rig.link.state() == State::Online);
    rig.sim.unplug();
    rig.run_until(secs(2), |rig| rig.link.state() == State::Lost);
    rig.sim.plug();
    rig.run(secs(5));
    assert_eq!(rig.link.state(), State::Lost, "no automatic retry");
    assert!(!rig.link.wants_open(rig.now));
    rig.link.request_reconnect(rig.now);
    assert!(rig.link.wants_open(rig.now));
    rig.run_until(secs(2), |rig| rig.link.state() == State::Online);
}

#[test]
fn app_02_a_failed_attempt_returns_to_lost_and_tries_again() {
    let mut rig = Rig::online();
    rig.sim.unplug();
    rig.run_until(secs(2), |rig| rig.link.state() == State::Lost);
    rig.events.clear();
    // Still unplugged: the attempt opens the port, gets no answer and fails.
    rig.run(secs(6));
    assert!(rig.events.is_empty(), "a failed retry is not a new loss");
    let connects = rig
        .written
        .iter()
        .filter(|(_, f)| f.command() == command::CONNECT)
        .count();
    assert!(
        connects >= 4,
        "at least two attempts of two connects: {connects}"
    );
    rig.sim.plug();
    rig.run_until(secs(6), |rig| rig.link.state() == State::Online);
}

#[test]
fn app_02_a_failed_open_during_reconnect_keeps_the_link_lost() {
    let mut rig = Rig::online();
    rig.link.io_error(rig.now, "gone");
    rig.link.take_events();
    rig.now += RECONNECT_INTERVAL;
    assert!(rig.link.wants_open(rig.now));
    rig.link.open_failed(
        rig.now,
        OpenError {
            reason: "busy".into(),
            permission_denied: false,
        },
    );
    assert_eq!(rig.link.state(), State::Lost);
    assert!(rig.link.take_events().is_empty());
    assert!(!rig.link.wants_open(rig.now));
    assert!(rig.link.wants_open(rig.now + RECONNECT_INTERVAL));
}

#[test]
fn app_11_the_core_is_deterministic() {
    let run = || {
        let mut rig = Rig::online();
        rig.sim.delay(Duration::from_millis(130));
        rig.run(secs(3));
        (rig.written.len(), rig.link.stats(), rig.link.version())
    };
    assert_eq!(run(), run());
}
