//! Behaviour of `SimDevice` against SIMULATOR.md. Time is injected; nothing
//! sleeps (SIM-02).

use std::time::Duration;

use protocol::{Dir, Frame, Reader, Register, command};
use simulator::{Command, Load, Outcome, Protection, SimDevice};

const MS: Duration = Duration::from_millis(1);
/// Comfortably past the write gap and the 5 ms step.
const GAP: Duration = Duration::from_millis(50);

struct Bench {
    sim: SimDevice,
    reader: Reader,
}

impl Bench {
    fn new() -> Self {
        Self {
            sim: SimDevice::new(),
            reader: Reader::new(Dir::DeviceToHost),
        }
    }

    /// A device with a session open.
    fn connected() -> Self {
        let mut bench = Self::new();
        bench.send(&Frame::connect());
        bench.sim.advance(GAP);
        bench.frames();
        bench
    }

    fn send(&mut self, frame: &Frame) {
        self.sim.feed(&frame.encode());
    }

    /// Everything the device sent, decoded; also checks every checksum.
    fn frames(&mut self) -> Vec<Frame> {
        let bytes = self.sim.take_output();
        let frames = self.reader.feed(self.sim.now(), &bytes);
        let stats = self.reader.stats();
        assert_eq!(
            stats.bad_checksum + stats.bad_length + stats.skipped_bytes,
            0
        );
        frames
    }

    /// A SET followed by the quiet gap.
    fn set(&mut self, id: u8, data: &[u8]) {
        let frame = Frame::new(Dir::HostToDevice, command::SET, id, data).unwrap();
        self.send(&frame);
        self.sim.advance(GAP);
    }

    fn set_float(&mut self, id: u8, value: f32) {
        self.set(id, &value.to_le_bytes());
    }

    /// GET one type and return the reply data.
    fn get(&mut self, register: Register) -> Option<Vec<u8>> {
        self.frames();
        self.send(&Frame::get(register));
        self.frames()
            .into_iter()
            .find(|frame| frame.id() == register.id())
            .map(|frame| frame.data().to_vec())
    }

    fn get_float(&mut self, register: Register) -> f32 {
        let data = self.get(register).expect("a reply");
        f32::from_le_bytes(data[..4].try_into().unwrap())
    }

    fn get_byte(&mut self, register: Register) -> u8 {
        self.get(register).expect("a reply")[0]
    }

    fn output(&mut self) -> (f32, f32, f32) {
        let data = self.get(Register::Output).expect("a reply");
        let at = |n: usize| f32::from_le_bytes(data[n..n + 4].try_into().unwrap());
        (at(0), at(4), at(8))
    }

    fn output_on(&mut self) {
        self.set(0xDB, &[1]);
    }

    fn output_off(&mut self) {
        self.set(0xDB, &[0]);
    }

    fn run(&mut self, line: &str) -> Outcome {
        self.sim.apply(line.parse::<Command>().unwrap())
    }
}

fn close_to(actual: f32, expected: f32) {
    assert!(
        (actual - expected).abs() < 0.001,
        "{actual} is not close to {expected}"
    );
}

// ---- session ---------------------------------------------------------------

#[test]
fn sim_05_device_is_silent_until_the_connect_frame() {
    let mut bench = Bench::new();
    bench.sim.advance(Duration::from_secs(2));
    assert!(bench.frames().is_empty(), "no push before connect");
    assert_eq!(bench.get(Register::InputVoltage), None);

    bench.send(&Frame::connect());
    bench.sim.advance(Duration::from_secs(1));
    assert!(!bench.frames().is_empty(), "push starts after connect");
    assert!(bench.get(Register::InputVoltage).is_some());
}

#[test]
fn sim_03_push_cycles_five_types_every_125_ms() {
    let mut bench = Bench::connected();
    bench.sim.advance(Duration::from_millis(125 * 10));
    let ids: Vec<u8> = bench.frames().iter().map(Frame::id).collect();
    assert_eq!(
        ids,
        [0xC3, 0xC0, 0xE2, 0xE3, 0xC4, 0xC3, 0xC0, 0xE2, 0xE3, 0xC4]
    );
}

#[test]
fn sim_15_connect_locks_and_close_unlocks_and_silences() {
    let mut bench = Bench::connected();
    assert!(bench.sim.is_locked());
    assert!(bench.run("state").eq(&Outcome::Text(bench.sim.state())));
    assert!(bench.sim.state().contains("locked true"));

    let close = Frame::new(Dir::HostToDevice, command::CONNECT, 0, [0]).unwrap();
    bench.send(&close);
    bench.sim.advance(Duration::from_secs(1));
    assert!(!bench.sim.is_locked());
    bench.frames();
    bench.sim.advance(Duration::from_secs(1));
    assert!(bench.frames().is_empty(), "silent again after close");
    assert_eq!(bench.get(Register::SetVoltage), None);
}

#[test]
fn sim_06_get_answers_known_types_and_ignores_the_rest() {
    let mut bench = Bench::connected();
    close_to(bench.get_float(Register::SetVoltage), 3.3);
    close_to(bench.get_float(Register::SetCurrent), 2.0);
    assert_eq!(bench.get(Register::Model).unwrap(), b"DPS-150");
    assert_eq!(bench.get(Register::HardwareVersion).unwrap(), b"V1.0");
    assert_eq!(bench.get(Register::FirmwareVersion).unwrap(), b"V1.2");
    bench.frames();
    bench.send(&Frame::new(Dir::HostToDevice, command::DATA, 0x42, [0]).unwrap());
    assert!(bench.frames().is_empty(), "an unknown type gets no reply");
}

#[test]
fn sim_06_dump_is_139_bytes_and_matches_the_single_gets() {
    let mut bench = Bench::connected();
    let dump = bench.get(Register::Dump).unwrap();
    assert_eq!(dump.len(), 139);
    let mut single = |register: Register| bench.get(register).unwrap();
    assert_eq!(dump[0..4], single(Register::InputVoltage)[..]);
    assert_eq!(dump[4..8], single(Register::SetVoltage)[..]);
    assert_eq!(dump[12..24], single(Register::Output)[..]);
    assert_eq!(dump[76..80], single(Register::Ovp)[..]);
    assert_eq!(dump[96], 3, "brightness");
    assert_eq!(dump[97], 1, "volume");
    assert_eq!(dump[98], 1, "D8");
    assert_eq!(dump[110], 1, "E1");
    assert_eq!(dump[135..139], single(Register::LvpRange)[..]);
}

#[test]
fn sim_04_initial_state_is_the_one_seen_on_the_unit() {
    let mut bench = Bench::connected();
    close_to(bench.get_float(Register::InputVoltage), 20.234);
    close_to(bench.get_float(Register::Temperature), 23.608);
    close_to(bench.get_float(Register::Ovp), 30.0);
    close_to(bench.get_float(Register::Ocp), 5.1);
    close_to(bench.get_float(Register::Opp), 150.0);
    close_to(bench.get_float(Register::Otp), 80.0);
    close_to(bench.get_float(Register::Lvp), 5.0);
    close_to(bench.get_float(Register::MaxVoltage), 20.034);
    close_to(bench.get_float(Register::MaxCurrent), 5.1);
    close_to(bench.get_float(Register::OtpRange), 99.0);
    assert_eq!(bench.get_byte(Register::OutputState), 0);
    assert_eq!(bench.get_byte(Register::Protection), 0);
    assert_eq!(bench.get_byte(Register::Mode), 1);
}

// ---- SET -------------------------------------------------------------------

#[test]
fn sim_07_set_is_stored_verbatim_without_validation() {
    let mut bench = Bench::connected();
    for value in [31.0f32, -1.0] {
        bench.set_float(0xC1, value);
        assert_eq!(bench.get_float(Register::SetVoltage), value);
    }
    bench.set_float(0xC2, 6.0);
    assert_eq!(bench.get_float(Register::SetCurrent), 6.0);
    bench.set_float(0xC5, 7.5);
    assert_eq!(bench.get_float(Register::from_id(0xC5).unwrap()), 7.5);
    bench.set_float(0xD0, 0.25);
    assert_eq!(bench.get_float(Register::from_id(0xD0).unwrap()), 0.25);
    bench.set_float(0xD4, 20.0);
    assert_eq!(bench.get_float(Register::Otp), 20.0);
    assert!(bench.frames().is_empty());
}

#[test]
fn sim_07_a_set_gets_no_acknowledgement() {
    let mut bench = Bench::connected();
    let frame = Frame::new(Dir::HostToDevice, command::SET, 0xC1, 5.0f32.to_le_bytes()).unwrap();
    bench.send(&frame);
    bench.sim.advance(GAP);
    // Pushes may arrive; a reply to the SET would be a C1 frame.
    assert!(bench.frames().iter().all(|frame| frame.id() != 0xC1));
}

#[test]
fn sim_13_set_to_a_read_only_type_changes_nothing() {
    let mut bench = Bench::connected();
    let before = bench.get(Register::Dump).unwrap();
    for id in [
        0xC0, 0xC3, 0xC4, 0xD8, 0xD9, 0xDA, 0xDC, 0xDD, 0xE1, 0xE2, 0xE3, 0xE4, 0xE8,
    ] {
        let len = Register::from_id(id).unwrap().data_len().unwrap();
        bench.set(id, &vec![0x55; len]);
    }
    for id in [0xDE, 0xDF, 0xE0] {
        bench.set(id, b"xx");
    }
    assert_eq!(bench.get(Register::Dump).unwrap(), before);
}

#[test]
fn sim_07_brightness_and_volume_keep_the_low_four_bits() {
    let mut bench = Bench::connected();
    for (written, read) in [(10u8, 10u8), (16, 0), (17, 1), (255, 15)] {
        bench.set(0xD6, &[written]);
        bench.set(0xD7, &[written]);
        assert_eq!(bench.get_byte(Register::Brightness), read);
        assert_eq!(bench.get_byte(Register::Volume), read);
    }
}

#[test]
fn sim_14_a_frame_right_after_a_set_makes_the_set_lost() {
    let mut bench = Bench::connected();
    let set = Frame::new(Dir::HostToDevice, command::SET, 0xC1, 9.0f32.to_le_bytes()).unwrap();
    let mut both = set.encode();
    both.extend(Frame::get(Register::SetVoltage).encode());
    bench.sim.feed(&both);
    let reply = bench.frames();
    let data = reply
        .iter()
        .find(|frame| frame.id() == 0xC1)
        .unwrap()
        .data();
    close_to(f32::from_le_bytes(data.try_into().unwrap()), 3.3);
    bench.sim.advance(GAP);
    assert_eq!(bench.sim.dropped(), 1);
    close_to(bench.get_float(Register::SetVoltage), 3.3);
}

#[test]
fn sim_14_a_frame_within_the_window_in_a_later_write_also_loses_the_set() {
    let mut bench = Bench::connected();
    bench.send(&Frame::new(Dir::HostToDevice, command::SET, 0xC1, 9.0f32.to_le_bytes()).unwrap());
    bench.sim.advance(MS);
    bench.send(&Frame::get(Register::SetVoltage));
    bench.sim.advance(GAP);
    close_to(bench.get_float(Register::SetVoltage), 3.3);
}

#[test]
fn sim_14_a_frame_after_the_window_leaves_the_set_alone() {
    let mut bench = Bench::connected();
    bench.send(&Frame::new(Dir::HostToDevice, command::SET, 0xC1, 9.0f32.to_le_bytes()).unwrap());
    bench.sim.advance(Duration::from_millis(3));
    bench.send(&Frame::get(Register::SetVoltage));
    let reply = bench.frames();
    let data = reply
        .iter()
        .find(|frame| frame.id() == 0xC1)
        .unwrap()
        .data();
    close_to(f32::from_le_bytes(data.try_into().unwrap()), 9.0);
    assert_eq!(bench.sim.dropped(), 0);
}

#[test]
fn sim_14_connect_and_close_frames_are_dropped_too() {
    let mut bench = Bench::new();
    let mut both = Frame::connect().encode();
    both.extend(Frame::get(Register::SetVoltage).encode());
    bench.sim.feed(&both);
    bench.sim.advance(GAP);
    assert!(
        !bench.sim.is_connected(),
        "connect followed at once is lost"
    );

    let mut bench = Bench::connected();
    let close = Frame::new(Dir::HostToDevice, command::CONNECT, 0, [0]).unwrap();
    let mut both = close.encode();
    both.extend(Frame::get(Register::SetVoltage).encode());
    bench.sim.feed(&both);
    bench.sim.advance(GAP);
    assert!(bench.sim.is_connected(), "close followed at once is lost");
}

#[test]
fn sim_02_frames_split_at_any_byte_and_joined_frames_are_accepted() {
    let mut bench = Bench::connected();
    let bytes = Frame::get(Register::SetCurrent).encode();
    for byte in &bytes {
        bench.sim.feed(&[*byte]);
    }
    assert!(bench.frames().iter().any(|frame| frame.id() == 0xC2));

    let mut joined = bytes.clone();
    joined.extend(Frame::get(Register::SetVoltage).encode());
    bench.sim.feed(&[0x00, 0x13]);
    bench.sim.feed(&joined);
    let ids: Vec<u8> = bench.frames().iter().map(Frame::id).collect();
    assert_eq!(ids, [0xC2, 0xC1]);
}

// ---- output ----------------------------------------------------------------

#[test]
fn sim_08_output_off_reads_zero() {
    let mut bench = Bench::connected();
    assert_eq!(bench.output(), (0.0, 0.0, 0.0));
}

#[test]
fn sim_08_cv_into_10_ohms_ramps_to_the_setpoint() {
    let mut bench = Bench::connected();
    bench.set_float(0xC1, 5.0);
    bench.set_float(0xC2, 2.0);
    bench.output_on();
    let (v, _, _) = bench.output();
    assert!(v > 0.0 && v < 5.0, "still ramping after 50 ms: {v}");
    bench.sim.advance(Duration::from_millis(1100));
    let (v, i, p) = bench.output();
    close_to(v, 5.0);
    close_to(i, 0.5);
    close_to(p, 2.5);
    assert_eq!(bench.get_byte(Register::Mode), 1, "CV");
    assert_eq!(bench.get_byte(Register::OutputState), 1);
}

#[test]
fn sim_08_cc_when_the_load_would_exceed_the_current_limit() {
    let mut bench = Bench::connected();
    bench.set_float(0xC1, 5.0);
    bench.set_float(0xC2, 0.2);
    bench.output_on();
    bench.sim.advance(Duration::from_millis(1100));
    let (v, i, p) = bench.output();
    close_to(v, 2.0);
    close_to(i, 0.2);
    close_to(p, 0.4);
    assert_eq!(bench.get_byte(Register::Mode), 0, "CC");
}

#[test]
fn sim_08_output_decays_in_about_half_a_second_after_off() {
    let mut bench = Bench::connected();
    bench.set_float(0xC1, 5.0);
    bench.output_on();
    bench.sim.advance(Duration::from_millis(1100));
    bench.output_off();
    let (v, _, _) = bench.output();
    assert!(v < 5.0 && v > 0.0, "decaying: {v}");
    bench.sim.advance(Duration::from_millis(600));
    assert_eq!(bench.output(), (0.0, 0.0, 0.0));
}

#[test]
fn sim_08_assumed_open_load_gives_no_current_and_cv() {
    let mut bench = Bench::connected();
    bench.run("load open");
    bench.set_float(0xC1, 5.0);
    bench.output_on();
    bench.sim.advance(Duration::from_millis(1100));
    let (v, i, p) = bench.output();
    close_to(v, 5.0);
    assert_eq!((i, p), (0.0, 0.0));
    assert_eq!(bench.get_byte(Register::Mode), 1);
}

#[test]
fn sim_08_voltage_above_the_live_maximum_is_clamped_without_a_trip() {
    let mut bench = Bench::connected();
    bench.run("load open");
    bench.set_float(0xC1, 25.0);
    bench.output_on();
    bench.sim.advance(Duration::from_millis(1100));
    let (v, _, _) = bench.output();
    close_to(v, 19.964); // E2 20.034 less the 0.07 V offset
    assert_eq!(bench.get_byte(Register::Protection), 0);
    close_to(bench.get_float(Register::SetVoltage), 25.0);
}

#[test]
fn sim_04_live_maximum_follows_the_input_voltage() {
    let mut bench = Bench::connected();
    bench.run("vin 15");
    close_to(bench.get_float(Register::InputVoltage), 15.0);
    close_to(bench.get_float(Register::MaxVoltage), 14.8);
}

#[test]
fn sim_04_capacity_and_energy_accumulate_while_on_and_stay_after_off() {
    let mut bench = Bench::connected();
    bench.set_float(0xC1, 5.0);
    bench.output_on();
    bench.sim.advance(Duration::from_secs(10));
    bench.output_off();
    bench.sim.advance(Duration::from_secs(5));
    let ah = bench.get_float(Register::Capacity);
    let wh = bench.get_float(Register::Energy);
    assert!(ah > 0.0011 && ah < 0.0015, "{ah}");
    assert!(wh > 0.0055 && wh < 0.0075, "{wh}");
    bench.sim.advance(Duration::from_secs(5));
    assert_eq!(bench.get_float(Register::Capacity), ah);
    bench.run("powercycle");
    bench.send(&Frame::connect());
    bench.sim.advance(GAP);
    assert_eq!(bench.get_float(Register::Capacity), 0.0);
    assert_eq!(bench.get_float(Register::Energy), 0.0);
}

// ---- protections -----------------------------------------------------------

fn tripped(bench: &mut Bench) -> u8 {
    bench.get_byte(Register::Protection)
}

#[test]
fn sim_10_ovp_trips_during_the_ramp_and_turns_the_output_off() {
    let mut bench = Bench::connected();
    bench.set_float(0xC1, 5.0);
    bench.set_float(0xD1, 4.5);
    bench.output_on();
    bench.sim.advance(Duration::from_secs(1));
    assert_eq!(tripped(&mut bench), 1);
    assert_eq!(bench.get_byte(Register::OutputState), 0);
    bench.sim.advance(Duration::from_secs(1));
    assert_eq!(bench.output(), (0.0, 0.0, 0.0));
}

#[test]
fn sim_10_each_protection_has_its_code() {
    let cases: [(u8, f32, u8); 3] = [(0xD2, 0.3, 2), (0xD3, 1.0, 3), (0xD4, 20.0, 4)];
    for (id, threshold, code) in cases {
        let mut bench = Bench::connected();
        bench.set_float(0xC1, 5.0);
        bench.set_float(id, threshold);
        bench.output_on();
        bench.sim.advance(Duration::from_secs(1));
        assert_eq!(tripped(&mut bench), code, "threshold {id:02X}");
    }
    let mut bench = Bench::connected();
    bench.set_float(0xD5, 25.0);
    assert_eq!(tripped(&mut bench), 5);
}

#[test]
fn sim_10_a_trip_is_latched_and_only_db_one_clears_it_and_turns_on() {
    let mut bench = Bench::connected();
    bench.set_float(0xC1, 5.0);
    bench.set_float(0xD1, 4.5);
    bench.output_on();
    bench.sim.advance(Duration::from_secs(1));
    assert_eq!(tripped(&mut bench), 1);

    bench.set_float(0xD1, 30.0);
    bench.sim.advance(Duration::from_secs(1));
    assert_eq!(
        tripped(&mut bench),
        1,
        "restoring the threshold does not clear"
    );
    bench.output_off();
    assert_eq!(tripped(&mut bench), 1, "DB = 0 does not clear");

    bench.output_on();
    assert_eq!(tripped(&mut bench), 0);
    assert_eq!(bench.get_byte(Register::OutputState), 1);
    bench.sim.advance(Duration::from_millis(1100));
    close_to(bench.output().0, 5.0);
}

#[test]
fn sim_10_equality_does_not_trip() {
    let mut bench = Bench::connected();
    bench.set_float(0xC1, 5.0);
    bench.set_float(0xD1, 5.0);
    bench.output_on();
    bench.sim.advance(Duration::from_secs(2));
    assert_eq!(tripped(&mut bench), 0);

    let mut bench = Bench::connected();
    bench.set_float(0xC1, 5.0);
    bench.set_float(0xC2, 0.2);
    bench.set_float(0xD2, 0.2);
    bench.output_on();
    bench.sim.advance(Duration::from_secs(2));
    assert_eq!(tripped(&mut bench), 0);
}

#[test]
fn sim_10_otp_and_lvp_trip_with_the_output_off_and_lvp_is_not_latched() {
    let mut bench = Bench::connected();
    bench.set_float(0xD4, 20.0);
    assert_eq!(tripped(&mut bench), 4, "OTP with output off");
    bench.set_float(0xD4, 80.0);
    assert_eq!(tripped(&mut bench), 4, "OTP stays latched");
    bench.output_on();
    assert_eq!(tripped(&mut bench), 0);
    bench.output_off();

    let mut bench = Bench::connected();
    bench.set_float(0xD5, 25.0);
    assert_eq!(tripped(&mut bench), 5);
    bench.set_float(0xD5, 5.0);
    assert_eq!(tripped(&mut bench), 0, "LVP clears by itself");
}

#[test]
fn sim_10_a_sag_below_lvp_trips_and_recovery_clears() {
    let mut bench = Bench::connected();
    bench.run("vin 4");
    bench.sim.advance(GAP);
    assert_eq!(tripped(&mut bench), 5);
    bench.run("vin 20");
    bench.sim.advance(GAP);
    assert_eq!(tripped(&mut bench), 0);
}

// ---- fault injection -------------------------------------------------------

#[test]
fn sim_11_trip_command_forces_a_protection_and_turns_the_output_off() {
    let mut bench = Bench::connected();
    bench.set_float(0xC1, 5.0);
    bench.output_on();
    bench.run("trip ocp");
    assert_eq!(tripped(&mut bench), 2);
    assert_eq!(bench.get_byte(Register::OutputState), 0);
}

#[test]
fn sim_11_keep_makes_the_device_hold_another_value() {
    let mut bench = Bench::connected();
    bench.run("keep C1 4.0");
    bench.set_float(0xC1, 5.0);
    close_to(bench.get_float(Register::SetVoltage), 4.0);
    bench.run("keep D6 2");
    bench.set(0xD6, &[7]);
    assert_eq!(bench.get_byte(Register::Brightness), 2);
}

#[test]
fn sim_11_unplug_goes_silent_keeps_the_output_and_needs_a_new_connect() {
    let mut bench = Bench::connected();
    bench.set_float(0xC1, 5.0);
    bench.output_on();
    bench.sim.advance(Duration::from_secs(1));
    bench.frames();

    bench.run("unplug");
    assert!(!bench.sim.is_locked());
    bench.sim.advance(Duration::from_secs(2));
    assert!(bench.frames().is_empty(), "silent while unplugged");
    bench.send(&Frame::get(Register::SetVoltage));
    assert!(bench.frames().is_empty(), "input is ignored");
    assert!(
        bench.sim.is_output_on(),
        "the cable pull leaves the output on"
    );

    bench.run("plug");
    bench.sim.advance(Duration::from_secs(1));
    assert!(bench.frames().is_empty(), "still silent without a connect");
    bench.send(&Frame::connect());
    bench.sim.advance(Duration::from_secs(1));
    assert!(!bench.frames().is_empty());
    assert_eq!(bench.get_byte(Register::OutputState), 1);
}

#[test]
fn sim_11_powercycle_keeps_settings_and_turns_the_output_off() {
    let mut bench = Bench::connected();
    bench.set_float(0xC1, 7.0);
    bench.output_on();
    bench.sim.advance(Duration::from_secs(1));
    bench.run("powercycle");
    assert!(!bench.sim.is_output_on());
    assert!(!bench.sim.is_locked());
    bench.sim.advance(Duration::from_secs(1));
    assert!(bench.frames().is_empty());
    bench.send(&Frame::connect());
    bench.sim.advance(GAP);
    close_to(bench.get_float(Register::SetVoltage), 7.0);
}

#[test]
fn sim_11_corrupt_gives_the_next_frames_a_wrong_checksum() {
    let mut bench = Bench::connected();
    bench.run("corrupt 2");
    bench.send(&Frame::get(Register::SetVoltage));
    bench.send(&Frame::get(Register::SetCurrent));
    bench.send(&Frame::get(Register::Ovp));
    let bytes = bench.sim.take_output();
    let mut reader = Reader::new(Dir::DeviceToHost);
    let frames = reader.feed(bench.sim.now(), &bytes);
    assert_eq!(frames.len(), 1, "two bad frames dropped, the third is good");
    assert_eq!(frames[0].id(), 0xD1);
    assert_eq!(reader.stats().bad_checksum, 2);
}

#[test]
fn sim_11_split_sends_a_frame_one_byte_at_a_time_20_ms_apart() {
    let mut bench = Bench::connected();
    bench.run("split 1");
    bench.send(&Frame::get(Register::SetVoltage));
    let mut reader = Reader::new(Dir::DeviceToHost);
    let mut frames = Vec::new();
    // The reply is 9 bytes, due at 0, 20, ... 160 ms. The first push is at
    // 125 ms after the connect, so count only the early bytes.
    let mut counts = vec![bench.sim.take_output()];
    for step in [40, 30] {
        bench.sim.advance(Duration::from_millis(step));
        counts.push(bench.sim.take_output());
    }
    assert_eq!(
        counts.iter().map(Vec::len).collect::<Vec<_>>(),
        [1, 2, 1],
        "bytes at 0, at 20 and 40, at 60 ms"
    );
    for bytes in &counts {
        frames.extend(reader.feed(bench.sim.now(), bytes));
    }
    assert!(frames.is_empty(), "the frame is not whole yet");
    bench.sim.advance(Duration::from_millis(150));
    let bytes = bench.sim.take_output();
    frames.extend(reader.feed(bench.sim.now(), &bytes));
    assert_eq!(frames.iter().filter(|frame| frame.id() == 0xC1).count(), 1);
    // Only one frame was split; the next one is whole again.
    bench.sim.advance(Duration::from_secs(1));
    bench.frames();
    bench.send(&Frame::get(Register::SetCurrent));
    assert_eq!(bench.sim.take_output().len(), 9);
}

#[test]
fn sim_11_delay_holds_every_reply_back() {
    let mut bench = Bench::connected();
    bench.run("delay 100");
    bench.send(&Frame::get(Register::SetVoltage));
    assert!(bench.sim.take_output().is_empty());
    bench.sim.advance(Duration::from_millis(99));
    assert!(
        bench.frames().iter().all(|frame| frame.id() != 0xC1),
        "not yet"
    );
    bench.sim.advance(Duration::from_millis(2));
    assert!(bench.frames().iter().any(|frame| frame.id() == 0xC1));
}

#[test]
fn sim_11_pty_commands_come_back_for_the_binary() {
    let mut bench = Bench::new();
    assert_eq!(bench.run("close"), Outcome::Close);
    assert_eq!(bench.run("reopen"), Outcome::Reopen);
    assert_eq!(bench.run("quit"), Outcome::Quit);
    assert_eq!(bench.run("load 4.7"), Outcome::Done);
    assert_eq!(bench.sim.measurement(), (0.0, 0.0, 0.0));
}

#[test]
fn sim_02_same_inputs_give_the_same_outputs() {
    let run = || {
        let mut bench = Bench::connected();
        bench.set_float(0xC1, 6.0);
        bench.output_on();
        bench.sim.advance(Duration::from_millis(777));
        let mut bytes = bench.sim.take_output();
        bytes.extend(bench.sim.state().bytes());
        bytes
    };
    assert_eq!(run(), run());
}

#[test]
fn sim_08_load_changes_take_effect_at_once() {
    let mut bench = Bench::connected();
    bench.set_float(0xC1, 5.0);
    bench.output_on();
    bench.sim.advance(Duration::from_millis(1100));
    bench.sim.set_load(Load::Ohms(5.0));
    close_to(bench.output().1, 1.0);
    assert_eq!(Protection::Lvp.code(), 5);
}
