//! The write path of the link core against `SimDevice` (level 1): the write
//! gap, coalescing, the second range check, read-back and loss.

mod common;

use std::time::Duration;

use common::{Rig, secs};
use device::{Event, Refusal, State};
use protocol::value::{MilliCelsius, Milliamps, Millivolts, Milliwatts};
use protocol::{Frame, Write, command};

fn volts(mv: u32) -> Write {
    Write::SetVoltage(Millivolts::new(mv))
}

fn amps(ma: u32) -> Write {
    Write::SetCurrent(Milliamps::new(ma))
}

fn sets(rig: &Rig) -> Vec<(Duration, Frame)> {
    rig.written
        .iter()
        .filter(|(_, f)| f.command() == command::SET)
        .cloned()
        .collect()
}

fn readbacks(rig: &Rig) -> Vec<(Write, Write)> {
    rig.events
        .iter()
        .filter_map(|e| match e {
            Event::ReadBack { written, device } => Some((*written, *device)),
            _ => None,
        })
        .collect()
}

#[test]
fn prt_05_nothing_is_written_unless_asked() {
    let mut rig = Rig::online();
    rig.run(secs(3));
    assert!(sets(&rig).is_empty());
    assert_eq!(rig.link.stats().sets_sent, 0);
    assert_eq!(rig.sim.set_voltage(), 3.3);
}

#[test]
fn lnk_03_a_write_before_the_first_dump_or_while_down_is_refused_and_sends_nothing() {
    let mut rig = Rig::new();
    assert_eq!(rig.link.write(volts(5000)), Err(Refusal::NotOnline));
    rig.run_until(secs(2), |r| r.link.state() == State::Online);
    rig.sim.unplug();
    rig.run_until(secs(2), |r| r.link.state() == State::Lost);
    rig.events.clear();
    assert_eq!(rig.link.write(volts(5000)), Err(Refusal::NotOnline));
    rig.sim.plug();
    rig.run(secs(4));
    assert!(
        sets(&rig).is_empty(),
        "nothing queued survives, nothing sent"
    );
    assert_eq!(rig.link.stats().refused_writes, 2);
    assert!(matches!(
        rig.events[0],
        Event::WriteRefused {
            why: Refusal::NotOnline,
            ..
        }
    ));
}

#[test]
fn lim_01_a_set_goes_out_once_the_device_takes_it_and_reads_back() {
    let mut rig = Rig::online();
    rig.link.write(volts(5000)).unwrap();
    rig.run(secs(1));
    assert_eq!(rig.sim.set_voltage(), 5.0);
    assert_eq!(rig.sim.dropped(), 0);
    let checked = readbacks(&rig);
    assert_eq!(checked, [(volts(5000), volts(5000))]);
    assert!(!checked[0].0.differs_from(checked[0].1));
    assert_eq!(rig.link.snapshot().unwrap().dump.set_voltage.get(), 5000);
}

#[test]
fn saf_15_nothing_follows_a_set_for_the_write_gap_and_the_link_stays_online() {
    let mut rig = Rig::online();
    rig.link.write(volts(5000)).unwrap();
    rig.run(secs(2));
    let (at, _) = sets(&rig)[0].clone();
    let next = rig
        .written
        .iter()
        .find(|(when, _)| *when > at)
        .map(|(when, _)| *when)
        .expect("polling resumes");
    assert!(next - at >= Duration::from_millis(100), "{:?}", next - at);
    assert_eq!(rig.sim.dropped(), 0, "the device kept the SET");
    assert_eq!(rig.link.state(), State::Online);
    assert!(
        rig.link.stats().quiet_ticks > 0,
        "ticks in the gap were quiet"
    );
}

#[test]
fn saf_15_polls_skipped_in_the_gap_do_not_count_towards_link_loss() {
    let mut rig = Rig::online();
    // A burst of writes to different registers keeps the line quiet for
    // longer than three poll periods.
    for write in [
        volts(5000),
        amps(1000),
        Write::Ovp(Millivolts::new(12_000)),
        Write::Ocp(Milliamps::new(3000)),
        Write::Opp(Milliwatts::new(100_000)),
        Write::Otp(MilliCelsius::new(70_000)),
    ] {
        rig.link.write(write).unwrap();
    }
    rig.run(secs(3));
    assert_eq!(rig.link.state(), State::Online);
    assert_eq!(rig.link.stats().missed_ticks, 0);
    assert_eq!(rig.link.stats().sets_sent, 6);
    assert_eq!(rig.sim.dropped(), 0);
    let times: Vec<Duration> = sets(&rig).iter().map(|(t, _)| *t).collect();
    for pair in times.windows(2) {
        assert!(pair[1] - pair[0] >= Duration::from_millis(100), "{pair:?}");
    }
    assert_eq!(readbacks(&rig).len(), 6);
}

#[test]
fn edt_03_writes_to_one_register_coalesce_to_the_latest_value() {
    let mut rig = Rig::online();
    for mv in [5000, 5010, 5020, 5030, 5040] {
        rig.link.write(volts(mv)).unwrap();
    }
    assert_eq!(rig.link.pending_writes(), [volts(5040)]);
    rig.run(secs(1));
    assert_eq!(rig.link.stats().sets_sent, 1);
    assert_eq!(rig.sim.set_voltage(), 5.04);
}

#[test]
fn edt_03_a_new_write_while_one_waits_for_its_gap_replaces_it() {
    let mut rig = Rig::online();
    rig.link.write(volts(5000)).unwrap();
    rig.run(Duration::from_millis(5)); // first SET is out, gap running
    rig.link.write(volts(6000)).unwrap();
    rig.link.write(volts(7000)).unwrap();
    rig.run(secs(1));
    assert_eq!(rig.link.stats().sets_sent, 2, "5.00 V, then only 7.00 V");
    assert_eq!(rig.sim.set_voltage(), 7.0);
}

#[test]
fn app_12_the_core_refuses_a_value_outside_the_device_range_on_its_own() {
    let mut rig = Rig::online();
    let bad = [
        volts(30_001),
        amps(5_101),
        Write::Ovp(Millivolts::new(30_001)),
        Write::Ocp(Milliamps::new(5_101)),
        Write::Opp(Milliwatts::new(150_001)),
        Write::Otp(MilliCelsius::new(99_001)),
        Write::Lvp(Millivolts::new(30_001)),
        Write::PresetVoltage(protocol::Slot::new(2).unwrap(), Millivolts::new(31_000)),
    ];
    for write in bad {
        assert_eq!(rig.link.write(write), Err(Refusal::OutOfRange), "{write:?}");
    }
    assert_eq!(rig.link.stats().refused_writes, bad.len() as u64);
    rig.run(secs(1));
    assert!(sets(&rig).is_empty());
    // The edges themselves are fine.
    for write in [
        volts(30_000),
        amps(5_100),
        Write::Opp(Milliwatts::new(150_000)),
    ] {
        assert_eq!(rig.link.write(write), Ok(()));
    }
}

#[test]
fn prt_03_the_read_back_shows_a_value_the_device_kept_instead() {
    let mut rig = Rig::online();
    rig.sim.keep(0xC1, 4.0);
    rig.link.write(volts(5000)).unwrap();
    rig.run(secs(1));
    let checked = readbacks(&rig);
    assert_eq!(checked, [(volts(5000), volts(4000))]);
    assert!(checked[0].0.differs_from(checked[0].1));
}

#[test]
fn prt_03_a_write_that_the_gap_rule_would_lose_is_seen_as_kept() {
    // The link never sends a frame inside the gap, so the simulator never
    // drops a SET; this guards that rule from the other side.
    let mut rig = Rig::online();
    for round in 0..5 {
        rig.link.write(volts(4000 + round * 100)).unwrap();
        rig.run(Duration::from_millis(350));
    }
    assert_eq!(rig.sim.dropped(), 0);
    assert_eq!(rig.sim.set_voltage(), 4.4);
}

#[test]
fn lnk_03_queued_writes_are_dropped_when_the_link_is_lost() {
    let mut rig = Rig::online();
    rig.sim.unplug();
    rig.link.write(volts(5000)).unwrap();
    rig.link.write(amps(1000)).unwrap();
    rig.run_until(secs(2), |r| r.link.state() == State::Lost);
    assert!(rig.link.pending_writes().is_empty());
    rig.sim.plug();
    rig.run_until(secs(4), |r| r.link.state() == State::Online);
    rig.run(secs(1));
    assert_eq!(
        rig.sim.set_voltage(),
        3.3,
        "the queued write never went out"
    );
    assert!(readbacks(&rig).is_empty());
}

#[test]
fn prt_04_the_value_on_the_wire_is_the_value_read_back_into_the_model() {
    let mut rig = Rig::online();
    rig.link.write(volts(12_340)).unwrap();
    rig.link.write(amps(2_345)).unwrap();
    rig.run(secs(1));
    let dump = rig.link.snapshot().unwrap().dump;
    assert_eq!(dump.set_voltage.get(), 12_340);
    assert_eq!(dump.set_current.get(), 2_345);
}

// ---- the output switch (M6) -----------------------------------------------------

fn on() -> Write {
    Write::Output(true)
}

fn off() -> Write {
    Write::Output(false)
}

#[test]
fn saf_01_the_core_sends_the_output_frames_and_reads_the_state_back() {
    let mut rig = Rig::online();
    rig.link.write(on()).unwrap();
    rig.run(secs(1));
    assert!(rig.sim.is_output_on());
    assert_eq!(readbacks(&rig), [(on(), on())]);
    assert!(rig.link.snapshot().unwrap().dump.output_on);
    rig.events.clear();
    rig.link.write(off()).unwrap();
    rig.run(secs(1));
    assert!(!rig.sim.is_output_on());
    assert_eq!(readbacks(&rig), [(off(), off())]);
    assert_eq!(rig.sim.dropped(), 0);
}

#[test]
fn app_09_an_off_goes_before_every_queued_setpoint() {
    let mut rig = Rig::online();
    rig.sim.panel_output(true);
    rig.run(secs(1));
    for write in [volts(5000), amps(1000), Write::Ovp(Millivolts::new(12_000))] {
        rig.link.write(write).unwrap();
    }
    rig.link.write(off()).unwrap();
    assert_eq!(rig.link.pending_writes()[0], off(), "first in the queue");
    rig.run(Duration::from_millis(50));
    assert!(!rig.sim.is_output_on(), "off goes out first");
    assert_eq!(rig.sim.set_voltage(), 3.3, "no setpoint has gone yet");
    rig.run(secs(1));
    assert_eq!(rig.sim.set_voltage(), 5.0, "they follow, one gap apart");
    let ids: Vec<u8> = sets(&rig).iter().map(|(_, f)| f.id()).collect();
    assert_eq!(ids, [0xDB, 0xC1, 0xC2, 0xD1]);
}

#[test]
fn app_09_an_off_is_not_merged_into_or_replaced_by_another_write() {
    let mut rig = Rig::online();
    rig.sim.panel_output(true);
    rig.run(secs(1));
    rig.link.write(off()).unwrap();
    rig.link.write(volts(5000)).unwrap();
    rig.link.write(off()).unwrap();
    let off_count = rig
        .link
        .pending_writes()
        .iter()
        .filter(|w| **w == off())
        .count();
    assert_eq!(off_count, 1, "a second OFF is the same OFF");
    assert_eq!(rig.link.pending_writes()[0], off());
    rig.run(secs(1));
    assert!(!rig.sim.is_output_on());
}

#[test]
fn saf_01_an_on_waits_behind_the_setpoints_the_user_saw() {
    let mut rig = Rig::online();
    rig.link.write(volts(6000)).unwrap();
    rig.link.write(amps(1200)).unwrap();
    rig.link.write(on()).unwrap();
    assert_eq!(rig.link.pending_writes().last(), Some(&on()));
    rig.run(secs(1));
    assert_eq!(rig.sim.set_voltage(), 6.0);
    assert!(rig.sim.is_output_on());
    // The order on the wire.
    let ids: Vec<u8> = sets(&rig).iter().map(|(_, f)| f.id()).collect();
    assert_eq!(ids, [0xC1, 0xC2, 0xDB]);
}

#[test]
fn saf_01_an_on_is_refused_while_an_off_is_queued_or_unconfirmed() {
    let mut rig = Rig::online();
    rig.sim.panel_output(true);
    rig.run(secs(1));
    rig.link.write(off()).unwrap();
    assert_eq!(rig.link.write(on()), Err(Refusal::OffPending), "queued");
    rig.run(Duration::from_millis(30)); // sent, not yet read back
    assert_eq!(
        rig.link.write(on()),
        Err(Refusal::OffPending),
        "unconfirmed"
    );
    rig.run(secs(1));
    assert_eq!(
        rig.link.write(on()),
        Ok(()),
        "confirmed: an ON is allowed again"
    );
}

#[test]
fn saf_01_a_queued_on_is_void_once_an_off_arrives() {
    let mut rig = Rig::online();
    rig.link.write(on()).unwrap();
    rig.link.write(off()).unwrap();
    assert_eq!(rig.link.pending_writes(), [off()]);
    rig.run(secs(1));
    assert!(!rig.sim.is_output_on());
}

#[test]
fn prt_03_an_off_that_did_not_take_shows_in_the_read_back() {
    let mut rig = Rig::online();
    rig.sim.panel_output(true);
    rig.run(secs(1));
    rig.sim.keep(0xDB, 1.0); // the device keeps the output on
    rig.link.write(off()).unwrap();
    rig.run(secs(1));
    assert_eq!(readbacks(&rig), [(off(), on())]);
    assert!(rig.sim.is_output_on());
}

#[test]
fn lnk_03_an_off_is_refused_like_any_write_while_the_link_is_down() {
    let mut rig = Rig::new();
    assert_eq!(rig.link.write(off()), Err(Refusal::NotOnline));
}
