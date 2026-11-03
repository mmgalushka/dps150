//! The output switch and its guards through the public API of `app`
//! (SAFETY.md §2, §3; DESIGN.md §1.5, §5.12).

use app::{Effect, Key, Model, Prompt, Safety, Tone};
use device::{Cause, Event, Message, Snapshot};
use protocol::value::{MilliCelsius, Milliamps, Millivolts, Milliwatts};
use protocol::{Dump, Mode, OutputReading, Preset, Write};

fn preset(volts: u32, amps: u32) -> Preset {
    Preset {
        voltage: Millivolts::new(volts),
        current: Milliamps::new(amps),
    }
}

/// 5.00 V / 1.500 A, live maximum 20.05 V / 5.10 A, all limits at default.
fn dump(output_on: bool) -> Dump {
    Dump {
        input_voltage: 20.25,
        set_voltage: Millivolts::new(5000),
        set_current: Milliamps::new(1500),
        output: OutputReading {
            voltage: 0.0,
            current: 0.0,
            power: 0.0,
        },
        temperature: 24.3,
        presets: [
            preset(3300, 1000),
            preset(5000, 1000),
            preset(9000, 2000),
            preset(12000, 2000),
            preset(5000, 500),
            preset(24000, 2000),
        ],
        ovp: Millivolts::new(30_000),
        ocp: Milliamps::new(5_100),
        opp: Milliwatts::new(150_000),
        otp: MilliCelsius::new(80_000),
        lvp: Millivolts::new(5_000),
        brightness: 3,
        volume: 1,
        flag_d8: 1,
        capacity_ah: 0.0,
        energy_wh: 0.0,
        output_on,
        protection: 0,
        mode: Mode::ConstantVoltage,
        flag_e1: 1,
        max_voltage: Millivolts::new(20_050),
        max_current: Milliamps::new(5_100),
        ovp_range: Millivolts::new(30_000),
        ocp_range: Milliamps::new(5_100),
        opp_range: Milliwatts::new(150_000),
        otp_range: MilliCelsius::new(99_000),
        lvp_range: Millivolts::new(30_000),
    }
}

fn snapshot(dump: Dump) -> Message {
    Message::Model(Box::new(Snapshot {
        dump,
        model: Some("DPS-150".into()),
        hardware: Some("V1.0".into()),
        firmware: Some("V1.2".into()),
    }))
}

fn model_with(dump: Dump) -> Model {
    let mut model = Model::new("/dev/x", true);
    model.apply(snapshot(dump));
    model
}

fn off() -> Model {
    model_with(dump(false))
}

fn on() -> Model {
    model_with(dump(true))
}

fn volts(mv: u32) -> Write {
    Write::SetVoltage(Millivolts::new(mv))
}

fn amps(ma: u32) -> Write {
    Write::SetCurrent(Milliamps::new(ma))
}

fn out(on: bool) -> Write {
    Write::Output(on)
}

fn text(model: &Model, compact: bool) -> String {
    model.message(compact).map(|l| l.text).unwrap_or_default()
}

fn type_text(model: &mut Model, text: &str) {
    for c in text.chars() {
        model.on_key(Key::Char(c));
    }
}

// ---- Space, ON and OFF ------------------------------------------------------------

#[test]
fn saf_01_space_turns_an_output_that_is_on_off_without_asking() {
    let mut model = on();
    assert_eq!(model.on_key(Key::Char(' ')), Effect::Write(out(false)));
    assert!(model.prompt().is_none(), "OFF never asks (SAF-04)");
}

#[test]
fn saf_03_space_on_an_output_that_is_off_asks_with_the_values_it_will_apply() {
    let mut model = off();
    assert_eq!(
        model.on_key(Key::Char(' ')),
        Effect::None,
        "nothing is sent yet"
    );
    assert!(matches!(model.prompt(), Some(Prompt::OutputOn { .. })));
    assert_eq!(
        model.prompt_text(false).unwrap(),
        "Turn output ON at 5.00 V / 1.500 A?"
    );
    assert_eq!(model.prompt_text(true).unwrap(), "ON at 5.00 V / 1.500 A?");
    assert_eq!(model.on_key(Key::Char('y')), Effect::Write(out(true)));
    assert!(model.prompt().is_none());
}

#[test]
fn saf_05_only_y_confirms_and_enter_and_other_keys_do_nothing() {
    for key in [
        Key::Enter,
        Key::Char('o'),
        Key::Char('Q'),
        Key::Up,
        Key::Tab,
        Key::Char('5'),
    ] {
        let mut model = off();
        model.on_key(Key::Char(' '));
        assert_eq!(model.on_key(key), Effect::None, "{key:?}");
        assert!(
            model.prompt().is_some(),
            "{key:?} must not answer or dismiss it"
        );
    }
}

#[test]
fn saf_05_n_and_esc_decline_and_send_nothing() {
    for key in [Key::Char('n'), Key::Esc] {
        let mut model = off();
        model.on_key(Key::Char(' '));
        assert_eq!(model.on_key(key), Effect::None);
        assert!(model.prompt().is_none());
    }
}

#[test]
fn saf_05_space_and_ctrl_x_cancel_a_prompt_and_turn_an_output_that_is_on_off() {
    // Output OFF: nothing to switch, the prompt is simply gone, and never ON.
    for key in [Key::Char(' '), Key::CtrlX] {
        let mut model = off();
        model.on_key(Key::Char(' '));
        assert_eq!(model.on_key(key), Effect::None, "{key:?}");
        assert!(model.prompt().is_none());
    }
    // Output ON with a jump prompt open: cancelled, and OFF is sent.
    for key in [Key::Char(' '), Key::CtrlX] {
        let mut model = on();
        type_text(&mut model, "15");
        model.on_key(Key::Enter);
        assert!(matches!(model.prompt(), Some(Prompt::Jump { .. })));
        assert_eq!(model.on_key(key), Effect::Write(out(false)), "{key:?}");
        assert!(model.prompt().is_none());
    }
}

#[test]
fn saf_01_ctrl_x_is_off_everywhere_and_never_on() {
    let mut model = off();
    assert_eq!(model.on_key(Key::CtrlX), Effect::None, "already off");
    assert!(model.prompt().is_none(), "Ctrl-X never asks to turn ON");
    let mut model = on();
    assert_eq!(model.on_key(Key::CtrlX), Effect::Write(out(false)));
    for screen_key in [Key::Char('p'), Key::Char('o')] {
        let mut model = on();
        model.on_key(screen_key);
        assert_eq!(model.on_key(Key::CtrlX), Effect::Write(out(false)));
    }
}

#[test]
fn saf_01_space_toggles_on_the_secondary_screens_too() {
    for screen_key in [Key::Char('p'), Key::Char('o')] {
        let mut model = on();
        model.on_key(screen_key);
        assert_eq!(model.on_key(Key::Char(' ')), Effect::Write(out(false)));
        let mut model = off();
        model.on_key(screen_key);
        assert_eq!(model.on_key(Key::Char(' ')), Effect::None);
        assert!(matches!(model.prompt(), Some(Prompt::OutputOn { .. })));
    }
}

#[test]
fn saf_01_in_the_rename_field_space_is_a_character_and_ctrl_x_still_switches_off() {
    let mut model = on();
    for key in [Key::Char('p'), Key::Char('n')] {
        model.on_key(key);
    }
    assert_eq!(model.on_key(Key::Char(' ')), Effect::None);
    assert!(model.prompt().is_none(), "Space typed a space");
    assert_eq!(model.on_key(Key::CtrlX), Effect::Write(out(false)));
    // The field is still open.
    assert_eq!(model.on_key(Key::Char('x')), Effect::None);
    model.on_key(Key::Enter);
    assert!(model.names()[0].ends_with(" x") || model.names()[0].contains(' '));
}

#[test]
fn saf_01_in_typed_entry_space_cancels_and_switches_an_output_that_is_on_off() {
    let mut model = on();
    model.on_key(Key::Enter);
    type_text(&mut model, "7");
    assert_eq!(model.on_key(Key::Char(' ')), Effect::Write(out(false)));
    assert!(model.entry().is_none());
    // Output OFF: it only cancels.
    let mut model = off();
    model.on_key(Key::Enter);
    assert_eq!(model.on_key(Key::Char(' ')), Effect::None);
    assert!(model.entry().is_none());
    assert!(model.prompt().is_none(), "never ON from a cancel");
    let mut model = on();
    model.on_key(Key::Enter);
    assert_eq!(model.on_key(Key::CtrlX), Effect::Write(out(false)));
}

#[test]
fn saf_01_space_cancels_the_recall_chord_and_switches_off() {
    let mut model = on();
    model.on_key(Key::Char('m'));
    assert_eq!(model.on_key(Key::Char(' ')), Effect::Write(out(false)));
    assert!(!model.chord());
}

#[test]
fn saf_03_with_output_on_immediately_the_prompt_is_skipped() {
    let mut model = off();
    model.set_safety(Safety {
        ask_on: false,
        ..Safety::default()
    });
    assert_eq!(model.on_key(Key::Char(' ')), Effect::Write(out(true)));
    assert!(model.prompt().is_none());
}

#[test]
fn saf_03_immediately_does_not_skip_the_ordering_rules() {
    let mut d = dump(false);
    d.ovp = Millivolts::new(5_000);
    let mut model = model_with(d);
    model.set_safety(Safety {
        ask_on: false,
        ..Safety::default()
    });
    assert_eq!(model.on_key(Key::Char(' ')), Effect::None);
    assert_eq!(
        text(&model, false),
        "Cannot turn ON: set voltage is not below OVP (o)"
    );
    assert_eq!(text(&model, true), "ON blocked: V not below OVP");
}

#[test]
fn lim_07_the_output_is_refused_on_above_the_live_maximum() {
    let mut d = dump(false);
    d.max_voltage = Millivolts::new(4_000);
    let mut model = model_with(d);
    assert_eq!(model.on_key(Key::Char(' ')), Effect::None);
    assert!(model.prompt().is_none());
    assert_eq!(
        text(&model, false),
        "Cannot turn ON: set voltage is above the live maximum"
    );
    assert_eq!(text(&model, true), "ON blocked: V above live max");
    // OFF and editing stay possible.
    let mut d = dump(true);
    d.max_voltage = Millivolts::new(4_000);
    let mut model = model_with(d);
    assert_eq!(model.on_key(Key::Char(' ')), Effect::Write(out(false)));
    let mut d = dump(false);
    d.max_current = Milliamps::new(1_000);
    let mut model = model_with(d);
    model.on_key(Key::Char(' '));
    assert_eq!(
        text(&model, false),
        "Cannot turn ON: set current is above the live maximum"
    );
}

#[test]
fn lim_06_a_stored_violation_refuses_on_with_a_notice() {
    let mut d = dump(false);
    d.ocp = Milliamps::new(1_500);
    let mut model = model_with(d);
    model.on_key(Key::Char(' '));
    assert_eq!(
        text(&model, false),
        "Cannot turn ON: set current is not below OCP (o)"
    );
    let mut d = dump(false);
    d.opp = Milliwatts::new(7_500);
    let mut model = model_with(d);
    model.on_key(Key::Char(' '));
    assert_eq!(
        text(&model, false),
        "Cannot turn ON: set power is not below OPP (o)"
    );
}

// ---- the jump guard ----------------------------------------------------------------------

#[test]
fn saf_06_a_change_beyond_the_threshold_asks_and_the_screen_keeps_the_old_values() {
    let mut model = on();
    type_text(&mut model, "15");
    assert_eq!(model.on_key(Key::Enter), Effect::None, "held");
    assert_eq!(
        model.prompt_text(false).unwrap(),
        "Set 15.00 V / 1.500 A (+10.00 V), output ON?"
    );
    assert_eq!(model.prompt_text(true).unwrap(), "Set 15.00 V (+10.00 V)?");
    assert_eq!(
        model.set_voltage(),
        Some(Millivolts::new(5000)),
        "nothing shown as changed"
    );
    assert_eq!(model.on_key(Key::Char('y')), Effect::Write(volts(15_000)));
    assert_eq!(model.set_voltage(), Some(Millivolts::new(15_000)));
}

#[test]
fn saf_06_a_drop_has_a_signed_delta() {
    let mut d = dump(true);
    d.set_voltage = Millivolts::new(12_000);
    let mut model = model_with(d);
    type_text(&mut model, "4");
    model.on_key(Key::Enter);
    assert_eq!(
        model.prompt_text(false).unwrap(),
        "Set 4.00 V / 1.500 A (-8.00 V), output ON?"
    );
}

#[test]
fn saf_06_the_current_has_its_own_threshold_and_text() {
    let mut model = on();
    model.on_key(Key::Char('i'));
    type_text(&mut model, "3.6");
    model.on_key(Key::Enter);
    assert_eq!(
        model.prompt_text(false).unwrap(),
        "Set 5.00 V / 3.600 A (+2.100 A), output ON?"
    );
    assert_eq!(model.prompt_text(true).unwrap(), "Set 3.600 A (+2.100 A)?");
}

#[test]
fn saf_06_the_threshold_is_strict_and_the_window_is_cumulative() {
    let mut model = on();
    // +1 V steps from 5.00: 6, 7, 8, 9, 10 are within 5.0 V of the reference.
    for expected in [6000, 7000, 8000, 9000, 10_000] {
        assert_eq!(
            model.on_key(Key::Up),
            Effect::Write(volts(expected)),
            "{expected}"
        );
    }
    // The next step makes it 6.0 V from the reference: it asks.
    assert_eq!(model.on_key(Key::Up), Effect::None);
    assert_eq!(
        model.prompt_text(false).unwrap(),
        "Set 11.00 V / 1.500 A (+6.00 V), output ON?"
    );
}

#[test]
fn saf_07_a_held_key_is_one_action_repeats_wait_for_the_answer() {
    let mut model = on();
    for _ in 0..5 {
        model.on_key(Key::Up);
    }
    model.on_key(Key::Up); // crosses: prompt
    assert!(model.prompt().is_some());
    // The repeats of a held key do nothing.
    for _ in 0..20 {
        assert_eq!(model.on_key(Key::Up), Effect::None);
    }
    assert!(model.prompt().is_some());
    assert_eq!(
        model.set_voltage(),
        Some(Millivolts::new(10_000)),
        "stopped at the threshold"
    );
}

#[test]
fn saf_06_after_a_yes_the_reference_moves_and_after_a_no_the_setpoint_stays() {
    let mut model = on();
    type_text(&mut model, "15");
    model.on_key(Key::Enter);
    model.on_key(Key::Char('y'));
    // Reference is now 15.00 V: another volt is nothing.
    assert_eq!(model.on_key(Key::Up), Effect::Write(volts(16_000)));
    // Declining leaves the setpoint where it was.
    let mut model = on();
    type_text(&mut model, "15");
    model.on_key(Key::Enter);
    assert_eq!(model.on_key(Key::Esc), Effect::None);
    assert_eq!(model.set_voltage(), Some(Millivolts::new(5000)));
    type_text(&mut model, "15");
    assert!(model.entry().is_some(), "typing starts a new entry");
}

#[test]
fn saf_06_while_the_output_is_off_the_reference_follows_the_setpoint() {
    let mut model = off();
    // A big move with the output off is no jump.
    type_text(&mut model, "15");
    assert_eq!(model.on_key(Key::Enter), Effect::Write(volts(15_000)));
    // The device now reports 15 V and turns ON from the panel: the reference
    // is what the device holds.
    let mut d = dump(true);
    d.set_voltage = Millivolts::new(15_000);
    model.apply(snapshot(d));
    assert_eq!(model.on_key(Key::Up), Effect::Write(volts(16_000)));
}

#[test]
fn saf_06_the_reference_is_the_setpoint_at_the_last_output_on() {
    let mut model = off();
    model.on_key(Key::Char(' '));
    model.on_key(Key::Char('y')); // ON at 5.00 V
    model.apply(snapshot(dump(true)));
    type_text(&mut model, "12");
    model.on_key(Key::Enter);
    assert!(
        matches!(model.prompt(), Some(Prompt::Jump { .. })),
        "7 V from 5.00 V"
    );
}

#[test]
fn saf_06_a_threshold_of_zero_turns_that_prompt_off() {
    let mut model = on();
    model.set_safety(Safety {
        v_jump: 0,
        i_jump: 2_000,
        ask_on: true,
    });
    type_text(&mut model, "20");
    assert_eq!(model.on_key(Key::Enter), Effect::Write(volts(20_000)));
    model.on_key(Key::Char('i'));
    type_text(&mut model, "4.5");
    model.on_key(Key::Enter);
    assert!(
        model.prompt().is_some(),
        "the current threshold still applies"
    );
}

#[test]
fn saf_06_a_prompt_for_both_quantities_names_both_changes() {
    // The device reports a setpoint far from the reference (panel edit).
    let mut model = on();
    let mut d = dump(true);
    d.set_voltage = Millivolts::new(12_000);
    d.set_current = Milliamps::new(4_000);
    model.apply(snapshot(d));
    // The reference stays at 5.00 V / 1.500 A while the output stays ON.
    model.on_key(Key::Up);
    assert_eq!(
        model.prompt_text(false).unwrap(),
        "Set 13.00 V / 4.000 A (+8.00 V, +2.500 A), output ON?"
    );
    assert_eq!(model.prompt_text(true).unwrap(), "Set 13.00 V, 4.000 A?");
}

#[test]
fn saf_06_a_small_step_at_a_time_never_asks_while_the_output_is_off() {
    let mut model = off();
    for _ in 0..12 {
        assert!(matches!(model.on_key(Key::Up), Effect::Write(_)));
    }
    assert!(model.prompt().is_none());
}

// ---- recall ---------------------------------------------------------------------------------

#[test]
fn saf_08_a_recall_with_the_output_on_always_asks_whatever_the_change() {
    let mut model = on();
    model.on_key(Key::Char('m'));
    assert_eq!(
        model.on_key(Key::Char('2')),
        Effect::None,
        "M2 is 5.00 V / 1.000 A"
    );
    assert_eq!(
        model.prompt_text(false).unwrap(),
        "Recall M2 5.00 V / 1.000 A, output ON?"
    );
    assert_eq!(model.prompt_text(true).unwrap(), "Recall M2 5.00V/1.000A?");
    assert_eq!(
        model.on_key(Key::Char('y')),
        Effect::Writes(vec![volts(5_000), amps(1_000)])
    );
}

#[test]
fn saf_08_a_recall_with_the_output_off_writes_at_once() {
    let mut model = off();
    model.on_key(Key::Char('m'));
    assert_eq!(
        model.on_key(Key::Char('3')),
        Effect::Writes(vec![volts(9_000), amps(2_000)])
    );
    assert!(model.prompt().is_none());
}

#[test]
fn saf_08_a_blocked_slot_is_refused_before_any_prompt() {
    let mut model = on();
    model.on_key(Key::Char('m'));
    assert_eq!(model.on_key(Key::Char('6')), Effect::None);
    assert!(model.prompt().is_none());
    assert!(text(&model, false).starts_with("M6 blocked:"));
}

// ---- the too-small screen -----------------------------------------------------------------------------

#[test]
fn saf_10_on_the_too_small_screen_only_off_and_ctrl_c_work() {
    let mut model = on();
    model.set_too_small(true);
    assert_eq!(model.on_key(Key::Char(' ')), Effect::Write(out(false)));
    assert_eq!(model.on_key(Key::CtrlX), Effect::Write(out(false)));
    assert_eq!(model.on_key(Key::Char('q')), Effect::None, "q is ignored");
    assert_eq!(model.on_key(Key::CtrlC), Effect::Quit);
    for key in [
        Key::Up,
        Key::Enter,
        Key::Char('p'),
        Key::Char('5'),
        Key::Tab,
    ] {
        assert_eq!(model.on_key(key), Effect::None, "{key:?}");
    }
    assert_eq!(model.screen(), app::Screen::Main);
}

#[test]
fn saf_10_space_never_turns_the_output_on_from_the_too_small_screen() {
    let mut model = off();
    model.set_too_small(true);
    assert_eq!(model.on_key(Key::Char(' ')), Effect::None);
    assert_eq!(model.on_key(Key::CtrlX), Effect::None);
    assert!(model.prompt().is_none());
}

#[test]
fn saf_09_becoming_too_small_cancels_the_prompt_and_the_entry_applying_nothing() {
    let mut model = on();
    type_text(&mut model, "15");
    model.on_key(Key::Enter);
    assert!(model.prompt().is_some());
    model.set_too_small(true);
    assert!(model.prompt().is_none());
    model.set_too_small(false);
    assert!(model.prompt().is_none(), "it does not come back");
    assert_eq!(model.set_voltage(), Some(Millivolts::new(5000)));
    model.on_key(Key::Char('7'));
    model.set_too_small(true);
    model.set_too_small(false);
    assert!(model.entry().is_none());
}

#[test]
fn saf_09_losing_the_link_cancels_a_prompt() {
    let mut model = off();
    model.on_key(Key::Char(' '));
    assert!(model.prompt().is_some());
    model.apply(Message::Event(Event::Lost(Cause::MissedPolls)));
    assert!(model.prompt().is_none());
}

// ---- link loss ------------------------------------------------------------------------------------------

fn lost(model: &mut Model) {
    model.apply(Message::Event(Event::Lost(Cause::MissedPolls)));
}

fn restored(model: &mut Model, dump: Dump) {
    model.apply(Message::Event(Event::Online { restored: true }));
    let _ = dump;
}

#[test]
fn lnk_04_space_while_down_queues_an_off_and_a_second_space_clears_it() {
    let mut model = on();
    lost(&mut model);
    assert_eq!(model.on_key(Key::Char(' ')), Effect::None);
    assert!(model.off_queued());
    assert_eq!(
        text(&model, false),
        "Off queued · use the front-panel button"
    );
    assert_eq!(model.on_key(Key::Char(' ')), Effect::None);
    assert!(!model.off_queued(), "toggled back");
    assert_eq!(text(&model, false), "Off queue cleared");
    // Ctrl-X only queues.
    model.on_key(Key::CtrlX);
    model.on_key(Key::CtrlX);
    assert!(model.off_queued());
}

#[test]
fn lnk_04_the_queued_off_is_the_first_write_after_the_reconnect() {
    let mut model = on();
    lost(&mut model);
    model.on_key(Key::Char(' '));
    // The device's own state arrives, then the event.
    model.apply(snapshot(dump(true)));
    restored(&mut model, dump(true));
    assert_eq!(model.drain_writes(), vec![out(false)]);
    assert!(!model.off_queued());
    assert_eq!(
        text(&model, false),
        "Link restored · queued OFF sent, output OFF"
    );
    assert_eq!(text(&model, true), "Restored · OFF sent");
    assert!(model.drain_writes().is_empty(), "sent once");
}

#[test]
fn lnk_07_without_a_queued_off_the_restore_notice_names_the_device_state() {
    let mut model = on();
    lost(&mut model);
    let mut d = dump(true);
    d.set_voltage = Millivolts::new(12_000);
    d.set_current = Milliamps::new(1_000);
    model.apply(snapshot(d.clone()));
    restored(&mut model, d);
    assert!(model.drain_writes().is_empty());
    assert_eq!(
        text(&model, false),
        "Link restored · output ON at 12.00 V / 1.000 A"
    );
    assert_eq!(text(&model, true), "Restored · ON 12.00 V / 1.000 A");
    let mut model = off();
    lost(&mut model);
    model.apply(snapshot(dump(false)));
    restored(&mut model, dump(false));
    assert_eq!(text(&model, false), "Link restored · output OFF");
}

#[test]
fn lnk_07_the_screen_shows_the_device_values_not_the_ones_from_before_the_loss() {
    let mut model = off();
    model.on_key(Key::Up); // 6.00 V pending
    lost(&mut model);
    assert_eq!(model.set_voltage(), Some(Millivolts::new(5000)));
    let mut d = dump(false);
    d.set_voltage = Millivolts::new(7_000);
    model.apply(snapshot(d.clone()));
    restored(&mut model, d);
    assert_eq!(model.set_voltage(), Some(Millivolts::new(7_000)));
}

#[test]
fn lnk_07_the_restore_notice_lasts_three_seconds() {
    let mut model = off();
    lost(&mut model);
    model.apply(snapshot(dump(false)));
    restored(&mut model, dump(false));
    model.tick(std::time::Duration::from_millis(2_900));
    assert!(model.message(false).is_some());
    model.tick(std::time::Duration::from_secs(3));
    assert!(model.message(false).is_none());
}

// ---- trips ---------------------------------------------------------------------------------------------------

fn tripped(code: u8) -> Model {
    let mut d = dump(false);
    d.protection = code;
    model_with(d)
}

#[test]
fn saf_13_space_on_a_tripped_output_offers_to_clear_it_and_turn_on() {
    let mut model = tripped(4);
    assert_eq!(model.tripped(), Some(app::protection::Limit::Otp));
    assert_eq!(model.on_key(Key::Char(' ')), Effect::None);
    assert!(matches!(model.prompt(), Some(Prompt::ClearTrip { .. })));
    assert_eq!(
        model.prompt_text(false).unwrap(),
        "Clear OTP and turn output ON at 5.00 V / 1.500 A?"
    );
    assert_eq!(
        model.prompt_text(true).unwrap(),
        "Clear OTP, ON at 5.00 V / 1.500 A?"
    );
    assert_eq!(model.on_key(Key::Char('y')), Effect::Write(out(true)));
}

#[test]
fn saf_13_a_trip_is_always_asked_about_even_with_output_on_immediately() {
    let mut model = tripped(2);
    model.set_safety(Safety {
        ask_on: false,
        ..Safety::default()
    });
    assert_eq!(model.on_key(Key::Char(' ')), Effect::None);
    assert!(matches!(model.prompt(), Some(Prompt::ClearTrip { .. })));
}

#[test]
fn saf_13_a_clear_obeys_the_ordering_rules_like_any_on() {
    let mut d = dump(false);
    d.protection = 1;
    d.ovp = Millivolts::new(5_000);
    let mut model = model_with(d);
    model.on_key(Key::Char(' '));
    assert!(model.prompt().is_none());
    assert_eq!(
        text(&model, false),
        "Cannot turn ON: set voltage is not below OVP (o)"
    );
}

#[test]
fn saf_13_the_prompt_says_when_the_cause_is_still_there() {
    let mut d = dump(false);
    d.protection = 4;
    d.temperature = 85.0;
    let mut model = model_with(d);
    model.on_key(Key::Char(' '));
    assert_eq!(
        model.prompt_text(false).unwrap(),
        "Clear OTP (85.0 °C > 80.0 °C) and turn ON at 5.00 V / 1.500 A?"
    );
    let mut d = dump(false);
    d.protection = 5;
    d.input_voltage = 4.0;
    let mut model = model_with(d);
    model.on_key(Key::Char(' '));
    assert!(
        model
            .prompt_text(false)
            .unwrap()
            .contains("4.00 V < 5.00 V")
    );
    assert!(model.prompt_text(false).unwrap().len() <= 76);
}

#[test]
fn saf_13_a_decline_changes_nothing_and_off_is_never_used_to_clear() {
    let mut model = tripped(3);
    model.on_key(Key::Char(' '));
    assert_eq!(model.on_key(Key::Char('n')), Effect::None);
    assert!(model.prompt().is_none());
    // The output is already off: Ctrl-X sends nothing, not a "clear".
    assert_eq!(model.on_key(Key::CtrlX), Effect::None);
}

#[test]
fn pro_01_the_model_knows_which_protection_tripped() {
    use app::protection::Limit;
    let cases = [
        (1, Limit::Ovp),
        (2, Limit::Ocp),
        (3, Limit::Opp),
        (4, Limit::Otp),
        (5, Limit::Lvp),
    ];
    for (code, limit) in cases {
        assert_eq!(tripped(code).tripped(), Some(limit));
    }
    assert_eq!(off().tripped(), None);
}

// ---- read-back failures -----------------------------------------------------------------------------------------

#[test]
fn prt_03_an_off_that_read_back_as_still_on_is_an_error_that_stays_on_screen() {
    let mut model = on();
    model.apply(Message::Event(Event::ReadBack {
        written: out(false),
        device: out(true),
    }));
    let line = model.message(false).unwrap();
    assert_eq!(line.tone, Tone::Error);
    assert_eq!(
        line.text,
        "Output did not turn OFF. Use the front-panel button"
    );
    assert_eq!(
        model.message(true).unwrap().text,
        "OFF failed: use the panel button"
    );
    model.tick(std::time::Duration::from_secs(60));
    assert!(model.message(false).is_some(), "it does not time out");
}

#[test]
fn prt_03_an_on_that_did_not_take_is_reported_as_kept() {
    let mut model = off();
    model.apply(Message::Event(Event::ReadBack {
        written: out(true),
        device: out(false),
    }));
    assert_eq!(text(&model, false), "Device kept output OFF");
}

#[test]
fn saf_01_a_refused_on_behind_an_off_is_explained() {
    let mut model = on();
    model.apply(Message::Event(Event::WriteRefused {
        write: out(true),
        why: device::Refusal::OffPending,
    }));
    assert_eq!(text(&model, false), "Output OFF is still pending");
}
