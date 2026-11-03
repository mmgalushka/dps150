//! The Presets and Protection screens through the public API of `app`.

use app::protection::Limit;
use app::{CLEARED, Effect, Focus, Key, Model, PresetMode, Prompt, ProtectionMode, Screen, Tone};
use device::{Cause, Event, Message, Snapshot};
use protocol::value::{MilliCelsius, Milliamps, Millivolts, Milliwatts};
use protocol::{Dump, Mode, OutputReading, Preset, Slot, Write};

fn preset(volts: u32, amps: u32) -> Preset {
    Preset {
        voltage: Millivolts::new(volts),
        current: Milliamps::new(amps),
    }
}

/// The example reading of DESIGN.md §0.2 with the output OFF.
fn dump() -> Dump {
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
        output_on: false,
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
    let mut names: app::Names = Default::default();
    for (slot, name) in [
        "3V3 logic",
        "USB 5V",
        "9V pedal",
        "12V fan",
        "LED test",
        "24V rail",
    ]
    .iter()
    .enumerate()
    {
        names[slot] = (*name).into();
    }
    model.set_names(names, true);
    model
}

fn model() -> Model {
    model_with(dump())
}

fn slot(n: u8) -> Slot {
    Slot::new(n).unwrap()
}

fn volts(mv: u32) -> Millivolts {
    Millivolts::new(mv)
}

fn amps(ma: u32) -> Milliamps {
    Milliamps::new(ma)
}

fn keys(model: &mut Model, keys: &[Key]) {
    for key in keys {
        model.on_key(*key);
    }
}

fn type_text(model: &mut Model, text: &str) {
    for c in text.chars() {
        model.on_key(Key::Char(c));
    }
}

// ---- navigation ----------------------------------------------------------------

#[test]
fn screens_p_and_o_open_presets_and_protection_and_esc_and_q_go_back() {
    let mut model = model();
    model.on_key(Key::Char('p'));
    assert_eq!(model.screen(), Screen::Presets);
    model.on_key(Key::Esc);
    assert_eq!(model.screen(), Screen::Main);
    model.on_key(Key::Char('o'));
    assert_eq!(model.screen(), Screen::Protection);
    model.on_key(Key::Char('q'));
    assert_eq!(
        model.screen(),
        Screen::Main,
        "q acts like Esc off the main panel"
    );
    assert_eq!(
        model.on_key(Key::Char('q')),
        Effect::Quit,
        "on the main panel it quits"
    );
}

#[test]
fn screens_ctrl_c_quits_from_every_screen() {
    for key in [Key::Char('p'), Key::Char('o')] {
        let mut model = model();
        model.on_key(key);
        assert_eq!(model.on_key(Key::CtrlC), Effect::Quit);
    }
}

// ---- recall -------------------------------------------------------------------

#[test]
fn pre_01_enter_on_the_list_recalls_v_and_i_and_never_touches_the_output() {
    let mut model = model();
    keys(&mut model, &[Key::Char('p'), Key::Down, Key::Down]);
    assert_eq!(model.presets_state().selected, 2);
    let effect = model.on_key(Key::Enter);
    assert_eq!(
        effect,
        Effect::Writes(vec![
            Write::SetVoltage(volts(9000)),
            Write::SetCurrent(amps(2000))
        ])
    );
    assert_eq!(model.set_voltage(), Some(volts(9000)), "shown at once");
    assert_eq!(
        model.message(false).unwrap().text,
        "Recalled M3 · 9.00 V / 2.000 A"
    );
    assert_eq!(
        model.message(true).unwrap().text,
        "Recalled M3 · 9.00V/2.000A"
    );
}

#[test]
fn edt_07_m_then_a_digit_recalls_from_the_main_panel() {
    let mut model = model();
    model.on_key(Key::Char('m'));
    assert!(model.chord());
    assert_eq!(
        model.message(false).unwrap().text,
        "Recall which preset? 1-6, Esc cancels"
    );
    assert_eq!(model.message(true).unwrap().text, "Recall which? 1-6, Esc");
    let effect = model.on_key(Key::Char('5'));
    assert_eq!(
        effect,
        Effect::Writes(vec![
            Write::SetVoltage(volts(5000)),
            Write::SetCurrent(amps(500))
        ])
    );
    assert!(!model.chord());
}

#[test]
fn edt_07_any_other_key_cancels_the_chord_and_is_consumed() {
    for key in [
        Key::Char('7'),
        Key::Char('x'),
        Key::Esc,
        Key::Up,
        Key::Char('0'),
    ] {
        let mut model = model();
        model.on_key(Key::Char('m'));
        assert_eq!(model.on_key(key), Effect::None, "{key:?}");
        assert!(!model.chord(), "{key:?}");
        assert_eq!(model.set_voltage(), Some(volts(5000)), "nothing changed");
    }
}

#[test]
fn pre_02_a_slot_above_the_live_maximum_is_blocked_with_a_notice_never_clamped() {
    let mut model = model();
    model.on_key(Key::Char('m'));
    assert_eq!(model.on_key(Key::Char('6')), Effect::None);
    assert_eq!(
        model.message(false).unwrap().text,
        "M6 blocked: 24.00 V exceeds the live maximum 20.05 V"
    );
    assert_eq!(
        model.message(true).unwrap().text,
        "M6 blocked: above 20.05 V"
    );
    assert_eq!(model.set_voltage(), Some(volts(5000)));
    let blocked = model.preset_blocked(5).unwrap();
    assert_eq!(blocked.standard, "24.00 V exceeds the live maximum 20.05 V");
    assert!(model.preset_blocked(2).is_none());
}

#[test]
fn pre_02_a_slot_that_breaks_an_ordering_rule_is_blocked() {
    let mut d = dump();
    d.ovp = volts(10_000);
    let mut model = model_with(d);
    assert!(
        model
            .preset_blocked(3)
            .unwrap()
            .standard
            .contains("is not below OVP 10.00 V")
    );
    model.on_key(Key::Char('m'));
    assert_eq!(model.on_key(Key::Char('4')), Effect::None);
    assert!(
        model
            .message(false)
            .unwrap()
            .text
            .starts_with("M4 blocked:")
    );
    // OPP: 12 V x 2 A = 24 W.
    let mut d = dump();
    d.opp = Milliwatts::new(20_000);
    let model = model_with(d);
    assert!(
        model
            .preset_blocked(3)
            .unwrap()
            .standard
            .contains("not below OPP 20.0 W")
    );
}

#[test]
fn lnk_03_with_the_link_down_nothing_can_be_recalled_saved_or_edited() {
    let mut model = model();
    model.on_key(Key::Char('p'));
    model.apply(Message::Event(Event::Lost(Cause::MissedPolls)));
    for key in [Key::Enter, Key::Char('e'), Key::Char('s'), Key::Char('x')] {
        assert_eq!(model.on_key(key), Effect::None);
        assert_eq!(
            model.message(false).unwrap().text,
            "Link down. Changes are not sent."
        );
    }
    assert!(matches!(model.presets_state().mode, PresetMode::List));
    assert!(model.prompt().is_none());
}

// ---- edit, save, clear, rename -------------------------------------------------

#[test]
fn pre_01_edit_mode_steps_the_slot_values_and_enter_writes_them() {
    let mut model = model();
    keys(&mut model, &[Key::Char('p'), Key::Char('e')]);
    let PresetMode::Edit(edit) = &model.presets_state().mode else {
        panic!("edit mode");
    };
    assert_eq!((edit.voltage, edit.current), (3300, 1000));
    // Voltage cursor starts on the units digit: +1 V.
    model.on_key(Key::Up);
    // Tab to the current, whose cursor is on the tenths: +0.1 A.
    model.on_key(Key::Tab);
    model.on_key(Key::Up);
    let effect = model.on_key(Key::Enter);
    assert_eq!(
        effect,
        Effect::Writes(vec![
            Write::PresetVoltage(slot(1), volts(4300)),
            Write::PresetCurrent(slot(1), amps(1100)),
        ])
    );
    assert!(matches!(model.presets_state().mode, PresetMode::List));
    assert_eq!(
        model.preset(0),
        Some((4300, 1100)),
        "shown before the read-back"
    );
    assert_eq!(
        model.set_voltage(),
        Some(volts(5000)),
        "the setpoints are untouched"
    );
}

#[test]
fn pre_01_esc_leaves_edit_mode_without_writing() {
    let mut model = model();
    keys(&mut model, &[Key::Char('p'), Key::Char('e'), Key::Up]);
    assert_eq!(model.on_key(Key::Esc), Effect::None);
    assert!(matches!(model.presets_state().mode, PresetMode::List));
    assert_eq!(model.preset(0), Some((3300, 1000)));
    assert_eq!(
        model.screen(),
        Screen::Presets,
        "Esc cancels the edit first"
    );
}

#[test]
fn lim_03_a_slot_edit_stops_at_the_device_range() {
    let mut d = dump();
    d.presets[0] = preset(29_500, 5_050);
    let mut model = model_with(d);
    keys(
        &mut model,
        &[Key::Char('p'), Key::Char('e'), Key::Left, Key::Up, Key::Up],
    );
    let PresetMode::Edit(edit) = &model.presets_state().mode else {
        panic!()
    };
    assert_eq!(edit.voltage, 30_000, "stops at 30.00 V, not 39.50 V");
    keys(&mut model, &[Key::Tab, Key::Left, Key::Up]);
    let PresetMode::Edit(edit) = &model.presets_state().mode else {
        panic!()
    };
    assert_eq!(edit.current, 5_100, "stops at the current limit");
}

#[test]
fn saf_12_save_asks_before_overwriting_and_y_writes_the_setpoints() {
    let mut model = model();
    keys(
        &mut model,
        &[Key::Char('p'), Key::Down, Key::Down, Key::Char('s')],
    );
    assert!(matches!(
        model.prompt(),
        Some(Prompt::Overwrite { slot: 2, .. })
    ));
    assert_eq!(
        model.prompt_text(false).unwrap(),
        "Overwrite M3?  9.00 V/2.000 A → 5.00 V/1.500 A"
    );
    assert_eq!(model.prompt_text(true).unwrap(), "Overwrite M3?");
    // Enter does not answer a prompt.
    assert_eq!(model.on_key(Key::Enter), Effect::None);
    assert!(model.prompt().is_some());
    assert_eq!(
        model.on_key(Key::Char('x')),
        Effect::None,
        "other keys are ignored"
    );
    let effect = model.on_key(Key::Char('y'));
    assert_eq!(
        effect,
        Effect::Writes(vec![
            Write::PresetVoltage(slot(3), volts(5000)),
            Write::PresetCurrent(slot(3), amps(1500)),
        ])
    );
    assert!(model.prompt().is_none());
}

#[test]
fn saf_12_save_into_a_slot_that_already_holds_the_values_does_not_ask() {
    let mut d = dump();
    d.presets[1] = preset(5000, 1500);
    let mut model = model_with(d);
    keys(&mut model, &[Key::Char('p'), Key::Down, Key::Char('s')]);
    assert!(model.prompt().is_none());
    assert_eq!(
        model.message(false).unwrap().text,
        "M2 already holds these values"
    );
}

#[test]
fn saf_12_n_and_esc_and_space_decline_a_prompt() {
    for key in [Key::Char('n'), Key::Esc, Key::Char(' '), Key::CtrlX] {
        let mut model = model();
        keys(&mut model, &[Key::Char('p'), Key::Char('s')]);
        assert!(model.prompt().is_some());
        assert_eq!(model.on_key(key), Effect::None, "{key:?}");
        assert!(model.prompt().is_none(), "{key:?}");
        assert_eq!(model.preset(0), Some((3300, 1000)));
        assert_eq!(model.screen(), Screen::Presets);
    }
}

#[test]
fn saf_12_clear_resets_the_slot_and_its_name_after_a_yes() {
    let mut model = model();
    keys(
        &mut model,
        &[
            Key::Char('p'),
            Key::Down,
            Key::Down,
            Key::Down,
            Key::Char('x'),
        ],
    );
    assert!(matches!(model.prompt(), Some(Prompt::Clear { slot: 3 })));
    assert_eq!(
        model.prompt_text(false).unwrap(),
        "Clear M4 \"12V fan\" (12.00 V / 2.000 A)?"
    );
    assert_eq!(model.prompt_text(true).unwrap(), "Clear M4?");
    let effect = model.on_key(Key::Char('y'));
    assert_eq!(
        effect,
        Effect::Writes(vec![
            Write::PresetVoltage(slot(4), volts(CLEARED.0)),
            Write::PresetCurrent(slot(4), amps(CLEARED.1)),
        ])
    );
    assert_eq!(model.names()[3], "");
    assert_eq!(model.take_names_to_save().unwrap()[3], "");
    assert!(model.take_names_to_save().is_none(), "asked once");
}

#[test]
fn saf_12_clearing_a_slot_that_is_already_clear_does_not_ask() {
    let mut d = dump();
    d.presets[0] = preset(5000, 1000);
    let mut model = model_with(d);
    model.set_names(Default::default(), true);
    keys(&mut model, &[Key::Char('p'), Key::Char('x')]);
    assert!(model.prompt().is_none());
    assert_eq!(model.message(false).unwrap().text, "M1 is already clear");
}

#[test]
fn saf_09_a_prompt_ends_with_the_link() {
    let mut model = model();
    keys(&mut model, &[Key::Char('p'), Key::Char('s')]);
    assert!(model.prompt().is_some());
    model.apply(Message::Event(Event::Lost(Cause::MissedPolls)));
    assert!(model.prompt().is_none());
}

#[test]
fn pre_04_rename_is_an_inline_field_enter_saves_esc_cancels_space_types() {
    let mut model = model();
    keys(&mut model, &[Key::Char('p'), Key::Down, Key::Char('n')]);
    let PresetMode::Rename(text) = &model.presets_state().mode else {
        panic!("rename mode");
    };
    assert_eq!(text, "USB 5V", "starts with the current name");
    keys(&mut model, &[Key::Backspace; 8]);
    type_text(&mut model, "my psu 12345678");
    let PresetMode::Rename(text) = &model.presets_state().mode else {
        panic!()
    };
    assert_eq!(text, "my psu 12", "a space is a character, nine at most");
    assert_eq!(model.on_key(Key::CtrlX), Effect::None);
    assert!(
        matches!(model.presets_state().mode, PresetMode::Rename(_)),
        "Ctrl-X is not a character"
    );
    model.on_key(Key::Enter);
    assert_eq!(model.names()[1], "my psu 12");
    assert_eq!(model.take_names_to_save().unwrap()[1], "my psu 12");
    // Esc cancels.
    keys(&mut model, &[Key::Char('n'), Key::Backspace, Key::Esc]);
    assert_eq!(model.names()[1], "my psu 12");
    assert!(model.take_names_to_save().is_none());
}

#[test]
fn pre_04_an_empty_name_clears_the_name() {
    let mut model = model();
    keys(&mut model, &[Key::Char('p'), Key::Char('n')]);
    keys(&mut model, &[Key::Backspace; 12]);
    model.on_key(Key::Enter);
    assert_eq!(model.names()[0], "");
}

#[test]
fn app_05_names_are_not_saved_when_presets_toml_is_invalid() {
    let mut model = model();
    model.set_names(Default::default(), false);
    keys(&mut model, &[Key::Char('p'), Key::Char('n')]);
    type_text(&mut model, "abc");
    model.on_key(Key::Enter);
    assert_eq!(model.names()[0], "abc", "applies to the session");
    assert!(model.take_names_to_save().is_none());
    assert_eq!(
        model.message(false).unwrap().text,
        "Names not saved: presets.toml is invalid"
    );
}

// ---- the Presets screen's own readings ----------------------------------------------

#[test]
fn pre_the_screen_knows_which_slot_matches_the_setpoints() {
    let mut d = dump();
    d.set_voltage = volts(9000);
    d.set_current = amps(2000);
    let model = model_with(d);
    assert_eq!(model.matching_preset(), Some(2));
    assert_eq!(model_with(dump()).matching_preset(), None);
}

#[test]
fn pre_02_the_edit_box_validation_warns_for_a_value_that_recall_would_block() {
    let model = model();
    assert_eq!(
        model.preset_validation(9000, 2000),
        Ok("within 20.05 V / 5.10 A".to_string())
    );
    assert_eq!(
        model.preset_validation(24_000, 2000),
        Err("24.00 V exceeds the live maximum 20.05 V. Recall is blocked.".to_string())
    );
}

// ---- Protection ---------------------------------------------------------------------

fn protection(model: &mut Model) {
    model.on_key(Key::Char('o'));
}

#[test]
fn pro_selecting_starts_on_ovp_and_arrows_move_between_the_five_rows() {
    let mut model = model();
    protection(&mut model);
    assert_eq!(model.protection_state().selected, 0);
    keys(
        &mut model,
        &[
            Key::Down,
            Key::Down,
            Key::Down,
            Key::Down,
            Key::Down,
            Key::Down,
        ],
    );
    assert_eq!(model.protection_state().selected, 4);
    keys(&mut model, &[Key::Up; 9]);
    assert_eq!(model.protection_state().selected, 0);
    assert!(matches!(
        model.protection_state().mode,
        ProtectionMode::Selecting
    ));
}

#[test]
fn pro_enter_edits_with_the_cursor_on_the_units_digit_and_arrows_step_the_candidate() {
    let mut model = model();
    protection(&mut model);
    keys(&mut model, &[Key::Down, Key::Enter]); // OCP, 5.10 A
    let ProtectionMode::Editing(edit) = &model.protection_state().mode else {
        panic!("editing");
    };
    assert_eq!((edit.candidate, edit.cursor), (5_100, 1));
    // Down one unit: 4.10 A. Nothing is written until Enter.
    assert_eq!(model.on_key(Key::Down), Effect::None);
    let ProtectionMode::Editing(edit) = &model.protection_state().mode else {
        panic!()
    };
    assert_eq!(edit.candidate, 4_100);
    // Up stops at the device range, 5.10 A.
    keys(&mut model, &[Key::Up, Key::Up, Key::Up]);
    let ProtectionMode::Editing(edit) = &model.protection_state().mode else {
        panic!()
    };
    assert_eq!(edit.candidate, 5_100);
}

#[test]
fn lim_05_enter_applies_a_valid_candidate_and_returns_to_selecting() {
    let mut model = model();
    protection(&mut model);
    keys(&mut model, &[Key::Down, Key::Enter, Key::Down]); // OCP -> 4.10 A
    let effect = model.on_key(Key::Enter);
    assert_eq!(effect, Effect::Write(Write::Ocp(amps(4_100))));
    assert!(matches!(
        model.protection_state().mode,
        ProtectionMode::Selecting
    ));
    assert_eq!(
        model.limit(Limit::Ocp),
        Some(4_100),
        "shown before the read-back"
    );
}

#[test]
fn lim_05_enter_is_disabled_while_the_candidate_is_invalid() {
    let mut model = model();
    protection(&mut model);
    // OCP 1.50 A would not be above the 1.500 A current limit.
    keys(&mut model, &[Key::Down, Key::Enter, Key::Left]); // tens digit
    keys(
        &mut model,
        &[Key::Right, Key::Down, Key::Down, Key::Down, Key::Down],
    ); // 5.10 -> 1.10 A
    let ProtectionMode::Editing(edit) = &model.protection_state().mode else {
        panic!()
    };
    assert_eq!(edit.candidate, 1_100);
    let check = model.protection_check(Limit::Ocp, 1_100).unwrap();
    assert!(!check.ok);
    assert_eq!(check.text, "OCP not above it");
    assert_eq!(check.compact, "not above limit 1.500 A");
    assert_eq!(model.on_key(Key::Enter), Effect::None, "disabled");
    assert!(
        matches!(model.protection_state().mode, ProtectionMode::Editing(_)),
        "stays in edit"
    );
    assert_eq!(model.limit(Limit::Ocp), Some(5_100));
}

#[test]
fn pro_esc_cancels_the_edit_then_leaves_the_screen() {
    let mut model = model();
    protection(&mut model);
    keys(&mut model, &[Key::Enter, Key::Down]);
    assert_eq!(model.on_key(Key::Esc), Effect::None);
    assert!(matches!(
        model.protection_state().mode,
        ProtectionMode::Selecting
    ));
    assert_eq!(model.screen(), Screen::Protection);
    assert_eq!(model.limit(Limit::Ovp), Some(30_000));
    model.on_key(Key::Esc);
    assert_eq!(model.screen(), Screen::Main);
}

#[test]
fn pro_r_loads_the_device_default_as_a_candidate_and_does_not_write() {
    let mut d = dump();
    d.ovp = volts(12_000);
    let mut model = model_with(d);
    protection(&mut model);
    assert_eq!(model.on_key(Key::Char('r')), Effect::None);
    let ProtectionMode::Editing(edit) = &model.protection_state().mode else {
        panic!()
    };
    assert_eq!(edit.candidate, 30_000);
    assert!(edit.from_default);
    assert_eq!(model.limit(Limit::Ovp), Some(12_000), "nothing written yet");
    assert_eq!(
        model.on_key(Key::Enter),
        Effect::Write(Write::Ovp(volts(30_000)))
    );
}

#[test]
fn pro_typing_a_digit_starts_typed_entry_and_enter_applies_it_directly() {
    let mut model = model();
    protection(&mut model);
    model.on_key(Key::Char('2'));
    type_text(&mut model, "5");
    let ProtectionMode::Editing(edit) = &model.protection_state().mode else {
        panic!()
    };
    assert_eq!(edit.typed.as_ref().unwrap().text, "25");
    assert_eq!(
        model.on_key(Key::Enter),
        Effect::Write(Write::Ovp(volts(25_000)))
    );
    assert!(matches!(
        model.protection_state().mode,
        ProtectionMode::Selecting
    ));
}

#[test]
fn lim_08_typed_out_of_range_or_invalid_text_is_rejected_with_the_reason() {
    let mut model = model();
    protection(&mut model);
    keys(&mut model, &[Key::Down, Key::Char('9')]); // OCP, typing 9
    assert_eq!(model.on_key(Key::Enter), Effect::None);
    let line = model.message(false).unwrap();
    assert_eq!(line.tone, Tone::Error);
    assert_eq!(line.text, "9 A: above range 0 – 5.10 A");
    let ProtectionMode::Editing(edit) = &model.protection_state().mode else {
        panic!()
    };
    assert!(edit.typed.as_ref().unwrap().rejected);
    // Below the current limit.
    model.on_key(Key::Backspace);
    type_text(&mut model, "1");
    model.on_key(Key::Enter);
    assert_eq!(
        model.message(false).unwrap().text,
        "1 A: not above limit 1.500 A"
    );
    // A valid one goes through.
    model.on_key(Key::Backspace);
    type_text(&mut model, "3.5");
    assert_eq!(
        model.on_key(Key::Enter),
        Effect::Write(Write::Ocp(amps(3_500)))
    );
}

#[test]
fn lim_03_the_typed_value_is_rounded_to_the_limit_resolution() {
    let mut model = model();
    protection(&mut model);
    keys(
        &mut model,
        &[Key::Down, Key::Down, Key::Char('9'), Key::Char('9')],
    ); // OPP, 99
    // OPP resolution is 0.1 W.
    type_text(&mut model, ".96");
    assert_eq!(
        model.on_key(Key::Enter),
        Effect::Write(Write::Opp(Milliwatts::new(100_000)))
    );
}

#[test]
fn lnk_03_protection_edits_are_refused_while_the_link_is_down() {
    let mut model = model();
    protection(&mut model);
    model.apply(Message::Event(Event::Lost(Cause::MissedPolls)));
    assert_eq!(model.on_key(Key::Enter), Effect::None);
    assert!(matches!(
        model.protection_state().mode,
        ProtectionMode::Selecting
    ));
    assert_eq!(
        model.message(false).unwrap().text,
        "Link down. Changes are not sent."
    );
}

#[test]
fn lim_06_an_existing_violation_is_shown_and_never_edited_by_the_app() {
    let mut d = dump();
    d.ovp = volts(5_000); // not above the 5.00 V setpoint
    let model = model_with(d);
    let ctx = model.protection_context().unwrap();
    let dump = &model.snapshot.as_ref().unwrap().dump;
    let (limit, text) = app::protection::violation(dump, &ctx).unwrap();
    assert_eq!(limit, Limit::Ovp);
    assert_eq!(text, "Set voltage 5.00 V is not below OVP 5.00 V");
    assert_eq!(model.limit(Limit::Ovp), Some(5_000), "left as it is");
}

#[test]
fn focus_stays_on_the_main_panel_while_a_secondary_screen_is_open() {
    let mut model = model();
    model.on_key(Key::Tab);
    model.on_key(Key::Char('p'));
    model.on_key(Key::Esc);
    assert_eq!(model.focus, Focus::Current);
}
