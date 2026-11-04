//! The help overlay's keys (DESIGN.md §5.13).

use app::{Effect, Key, Model, Prompt, Screen};
use device::{Cause, Event, Message, Snapshot};
use protocol::value::{MilliCelsius, Milliamps, Millivolts, Milliwatts};
use protocol::{Dump, Mode, OutputReading, Preset, Write};

fn preset() -> Preset {
    Preset {
        voltage: Millivolts::new(5000),
        current: Milliamps::new(1000),
    }
}

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
        presets: [preset(); 6],
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

fn model(output_on: bool) -> Model {
    let mut model = Model::new("/dev/x", true);
    model.apply(Message::Model(Box::new(Snapshot {
        dump: dump(output_on),
        model: Some("DPS-150".into()),
        hardware: Some("V1.0".into()),
        firmware: Some("V1.2".into()),
    })));
    model
}

fn open(model: &mut Model) {
    assert_eq!(model.on_key(Key::Char('?')), Effect::None);
    assert!(model.help_page().is_some(), "the overlay should be open");
}

#[test]
fn ui_08_question_mark_opens_help_on_every_screen_and_esc_q_and_question_mark_close_it() {
    for screen_key in [None, Some(Key::Char('p')), Some(Key::Char('o'))] {
        for close in [Key::Esc, Key::Char('q'), Key::Char('?')] {
            let mut model = model(false);
            if let Some(key) = screen_key {
                model.on_key(key);
            }
            let screen = model.screen();
            open(&mut model);
            assert_eq!(model.on_key(close), Effect::None, "{close:?} only closes");
            assert!(model.help_page().is_none(), "{close:?}");
            assert_eq!(
                model.screen(),
                screen,
                "{close:?} does not leave the screen"
            );
            assert!(model.prompt().is_none());
        }
    }
}

#[test]
fn ui_08_q_in_the_overlay_never_quits_or_asks() {
    let mut model = model(true);
    open(&mut model);
    assert_eq!(model.on_key(Key::Char('q')), Effect::None);
    assert!(
        model.prompt().is_none(),
        "no quit prompt: it only closed the overlay"
    );
}

#[test]
fn ui_08_other_keys_do_nothing_while_the_overlay_is_open() {
    let mut model = model(false);
    open(&mut model);
    for key in [
        Key::Enter,
        Key::Tab,
        Key::Left,
        Key::Right,
        Key::Char('5'),
        Key::Char('v'),
        Key::Char('p'),
        Key::Char('o'),
        Key::Char('m'),
        Key::Backspace,
    ] {
        assert_eq!(model.on_key(key), Effect::None, "{key:?}");
        assert!(model.help_page().is_some(), "{key:?} must not close it");
    }
    assert_eq!(model.screen(), Screen::Main);
    assert!(model.entry().is_none());
    assert!(!model.chord());
    assert_eq!(model.set_voltage(), Some(Millivolts::new(5000)));
}

#[test]
fn ui_08_the_pages_of_the_compact_overlay_turn_with_up_and_down_and_stop_at_zero() {
    let mut model = model(false);
    open(&mut model);
    assert_eq!(model.help_page(), Some(0));
    model.on_key(Key::Up);
    assert_eq!(model.help_page(), Some(0));
    model.on_key(Key::Down);
    assert_eq!(model.help_page(), Some(1));
    model.on_key(Key::Up);
    assert_eq!(model.help_page(), Some(0));
    for _ in 0..30 {
        model.on_key(Key::Down);
    }
    let last = app::compact_pages(Screen::Main) - 1;
    assert_eq!(model.help_page(), Some(last), "stops on the last page");
    model.on_key(Key::Up);
    assert_eq!(
        model.help_page(),
        Some(last - 1),
        "and one Up goes back one page"
    );
    // Reopening starts at the first page.
    model.on_key(Key::Esc);
    open(&mut model);
    assert_eq!(model.help_page(), Some(0));
}

#[test]
fn saf_01_space_and_ctrl_x_still_turn_the_output_off_under_the_overlay() {
    for key in [Key::Char(' '), Key::CtrlX] {
        let mut model = model(true);
        open(&mut model);
        assert_eq!(
            model.on_key(key),
            Effect::Write(Write::Output(false)),
            "{key:?}"
        );
        assert!(model.help_page().is_some(), "the overlay stays");
    }
}

#[test]
fn saf_01_space_never_turns_the_output_on_from_under_the_overlay() {
    let mut model = model(false);
    open(&mut model);
    assert_eq!(model.on_key(Key::Char(' ')), Effect::None);
    assert!(model.prompt().is_none(), "the question could not be seen");
    assert_eq!(model.on_key(Key::CtrlX), Effect::None);
}

#[test]
fn saf_10_ctrl_c_quits_from_the_overlay() {
    let mut model = model(true);
    open(&mut model);
    assert_eq!(model.on_key(Key::CtrlC), Effect::Quit);
}

#[test]
fn ui_08_question_mark_is_not_help_where_it_is_a_typed_character_or_a_question_waits() {
    // Typed entry.
    let mut model = model(false);
    model.on_key(Key::Enter);
    model.on_key(Key::Char('?'));
    assert!(model.help_page().is_none());
    assert!(model.entry().is_some());
    // A prompt.
    let mut model = self::model(false);
    model.on_key(Key::Char(' '));
    assert!(matches!(model.prompt(), Some(Prompt::OutputOn { .. })));
    model.on_key(Key::Char('?'));
    assert!(model.help_page().is_none());
    assert!(model.prompt().is_some());
    // The recall chord.
    let mut model = self::model(false);
    model.on_key(Key::Char('m'));
    model.on_key(Key::Char('?'));
    assert!(model.help_page().is_none());
    // The rename field takes it as a character.
    let mut model = self::model(false);
    for key in [Key::Char('p'), Key::Char('n'), Key::Char('?')] {
        model.on_key(key);
    }
    assert!(model.help_page().is_none());
    model.on_key(Key::Enter);
    assert!(model.names()[0].ends_with('?'));
    // Typed entry on the Protection screen.
    let mut model = self::model(false);
    for key in [Key::Char('o'), Key::Char('5'), Key::Char('?')] {
        model.on_key(key);
    }
    assert!(model.help_page().is_none());
}

#[test]
fn ui_08_it_opens_in_the_middle_of_a_digit_edit_and_leaves_the_edit_alone() {
    let mut model = model(false);
    for key in [Key::Char('o'), Key::Enter, Key::Down] {
        model.on_key(key);
    }
    open(&mut model);
    model.on_key(Key::Esc);
    // Still editing, candidate kept: Enter applies it.
    assert!(matches!(
        model.on_key(Key::Enter),
        Effect::Write(Write::Ovp(_))
    ));
}

#[test]
fn saf_09_becoming_too_small_closes_the_overlay_and_a_link_loss_leaves_it() {
    let mut model = model(false);
    open(&mut model);
    model.set_too_small(true);
    assert!(model.help_page().is_none());
    // A link loss does not need to close a read-only overlay.
    let mut model = self::model(false);
    open(&mut model);
    model.apply(Message::Event(Event::Lost(Cause::MissedPolls)));
    assert!(model.help_page().is_some());
    assert_eq!(model.on_key(Key::Esc), Effect::None);
    assert!(model.help_page().is_none());
}
