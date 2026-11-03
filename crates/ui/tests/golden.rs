//! Level 3 (DEVOPS.md §4): renders the main panel at the reference sizes and
//! compares characters with the files in `tests/golden/` (UI-02). Colour is
//! not compared here.

use app::{Key, Model};
use device::{Cause, Event, Message, Snapshot};
use protocol::value::{MilliCelsius, Milliamps, Millivolts, Milliwatts};
use protocol::{Dump, Mode, OutputReading, Preset};
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ui::{Depth, Theme, render};

fn preset(volts: u32, amps: u32) -> Preset {
    Preset {
        voltage: Millivolts::new(volts),
        current: Milliamps::new(amps),
    }
}

/// The example reading of DESIGN.md §0.2: output ON, CV.
fn example_dump() -> Dump {
    Dump {
        input_voltage: 20.25,
        set_voltage: Millivolts::new(5000),
        set_current: Milliamps::new(1500),
        output: OutputReading {
            voltage: 4.998,
            current: 0.512,
            power: 2.56,
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
        ovp: Millivolts::new(30000),
        ocp: Milliamps::new(5100),
        opp: Milliwatts::new(150000),
        otp: MilliCelsius::new(80000),
        lvp: Millivolts::new(5000),
        brightness: 3,
        volume: 1,
        flag_d8: 1,
        capacity_ah: 0.012,
        energy_wh: 0.060,
        output_on: true,
        protection: 0,
        mode: Mode::ConstantVoltage,
        flag_e1: 1,
        max_voltage: Millivolts::new(20050),
        max_current: Milliamps::new(5100),
        ovp_range: Millivolts::new(30000),
        ocp_range: Milliamps::new(5100),
        opp_range: Milliwatts::new(150000),
        otp_range: MilliCelsius::new(99000),
        lvp_range: Millivolts::new(30000),
    }
}

fn cc_dump() -> Dump {
    let mut dump = example_dump();
    dump.output = OutputReading {
        voltage: 3.12,
        current: 1.5,
        power: 4.68,
    };
    dump.mode = Mode::ConstantCurrent;
    dump
}

fn model_with(dump: Dump, port: &str) -> Model {
    let mut model = Model::new(port, true);
    model.apply(Message::Model(Box::new(Snapshot {
        dump,
        model: Some("DPS-150".into()),
        hardware: Some("V1.0".into()),
        firmware: Some("V1.2".into()),
    })));
    model
}

fn lost(mut model: Model) -> Model {
    model.apply(Message::Event(Event::Lost(Cause::MissedPolls)));
    model
}

const PORT: &str = "/dev/cu.usbmodem13C85FCE40251";

/// The rendered rows with trailing spaces removed.
fn rows(buf: &Buffer) -> Vec<String> {
    (0..buf.area.height)
        .map(|y| {
            let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
            line.trim_end().to_string()
        })
        .collect()
}

fn golden(name: &str) -> Vec<String> {
    let path = format!(
        "{}/../../tests/golden/{name}.txt",
        env!("CARGO_MANIFEST_DIR")
    );
    std::fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("{path}: {error}"))
        .lines()
        .map(|line| line.trim_end().to_string())
        .collect()
}

fn draw(model: &Model, cols: u16, rows_: u16) -> Vec<String> {
    let mut buf = Buffer::empty(Rect::new(0, 0, cols, rows_));
    render(model, &mut buf, Theme::new(Depth::TrueColor));
    rows(&buf)
}

/// Compares a rendering with a golden file; the first difference is named by
/// row and column. Rows the file does not have must be blank.
fn check(name: &str, model: &Model, cols: u16, rows_: u16) {
    let got = draw(model, cols, rows_);
    let want = golden(name);
    for (y, got_line) in got.iter().enumerate() {
        let want_line = want.get(y).map_or("", String::as_str);
        if got_line != want_line {
            let column = got_line
                .chars()
                .zip(want_line.chars())
                .position(|(a, b)| a != b)
                .unwrap_or_else(|| got_line.chars().count().min(want_line.chars().count()));
            panic!(
                "{name}: row {y}, column {column} differs\n  want: {want_line:?}\n   got: {got_line:?}"
            );
        }
    }
    assert!(
        want.len() <= got.len(),
        "{name}: the file has more rows than the screen"
    );
}

/// Types `text` on the voltage readout, as the keys would.
fn typing(mut model: Model, text: &str, apply: bool) -> Model {
    model.on_key(Key::Enter);
    for c in text.chars() {
        model.on_key(Key::Char(c));
    }
    if apply {
        model.on_key(Key::Enter);
    }
    model
}

/// The example names of DESIGN.md §0.2.
fn named(mut model: Model) -> Model {
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

fn presets_screen() -> Model {
    let mut model = named(model_with(example_dump(), PORT));
    // M3 is selected in every Presets reference.
    for key in [Key::Char('p'), Key::Down, Key::Down] {
        model.on_key(key);
    }
    model
}

fn protection_screen() -> Model {
    let mut model = named(model_with(example_dump(), PORT));
    // Editing OCP, cursor on the units digit.
    for key in [Key::Char('o'), Key::Down, Key::Enter] {
        model.on_key(key);
    }
    model
}

#[test]
fn ui_02_presets_standard() {
    check("presets-standard", &presets_screen(), 80, 24);
}

#[test]
fn ui_02_presets_compact() {
    check("presets-compact", &presets_screen(), 44, 16);
}

#[test]
fn ui_02_presets_120x40() {
    check("presets-120x40", &presets_screen(), 120, 40);
}

#[test]
fn ui_02_protection_standard() {
    check("protection-standard", &protection_screen(), 80, 24);
}

#[test]
fn ui_02_protection_compact() {
    check("protection-compact", &protection_screen(), 44, 16);
}

#[test]
fn ui_02_protection_120x40() {
    check("protection-120x40", &protection_screen(), 120, 40);
}

#[test]
fn ui_02_main_standard_entry() {
    let model = typing(model_with(example_dump(), PORT), "12.5", false);
    check("main-standard-entry", &model, 80, 24);
}

#[test]
fn ui_02_main_standard_reject() {
    let model = typing(model_with(example_dump(), PORT), "35", true);
    check("main-standard-reject", &model, 80, 24);
}

#[test]
fn ui_02_main_compact_entry() {
    let model = typing(model_with(example_dump(), PORT), "12.5", false);
    check("main-compact-entry", &model, 44, 16);
}

#[test]
fn ui_05_a_rejected_entry_turns_its_text_red_and_the_message_is_an_error() {
    let model = typing(model_with(example_dump(), PORT), "35", true);
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 24));
    render(&model, &mut buf, Theme::new(Depth::TrueColor));
    let (x, y) = find(&buf, "35 V is above");
    assert_eq!(buf[(x, y)].fg, rgb(0xff5c7a), "message in er");
    let set = find(&buf, "SET  ←→");
    assert_eq!(
        buf[(set.0 + 3, set.1 + 2)].fg,
        rgb(0xff5c7a),
        "typed digits in er"
    );
    // A normal entry stays gold.
    let model = typing(model_with(example_dump(), PORT), "12.5", false);
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 24));
    render(&model, &mut buf, Theme::new(Depth::TrueColor));
    let set = find(&buf, "SET  ←→");
    assert_eq!(buf[(set.0 + 3, set.1 + 2)].fg, rgb(0xf6c177));
}

#[test]
fn ui_08_while_typing_the_key_bar_offers_apply_cancel_and_off_and_no_help() {
    let model = typing(model_with(example_dump(), PORT), "5", false);
    for (cols_, rows_) in [(44, 16), (80, 24), (120, 40)] {
        let last = draw(&model, cols_, rows_);
        let bar = last
            .iter()
            .rev()
            .find(|l| l.contains("Esc  cancel"))
            .expect("a key bar");
        assert!(
            bar.contains("Esc  cancel") && bar.contains("Space  off"),
            "{bar}"
        );
        assert!(!bar.contains("help"), "{cols_}x{rows_}: {bar}");
    }
}

#[test]
fn edt_04_a_pending_edit_shows_before_the_device_confirms_it() {
    // Output OFF, so the edit is sent; the screen shows the new set value.
    let mut dump = example_dump();
    dump.output_on = false;
    let mut model = model_with(dump, PORT);
    model.on_key(Key::Up); // +1 V on the units digit
    let lines = draw(&model, 80, 24);
    // 05.00 has `┗━┓` under its 5; 06.00 has `┣━┓` under its 6.
    assert!(
        lines.iter().any(|l| l.contains("┃ ┃ ┣━┓")),
        "06.00 in big digits"
    );
}

#[test]
fn lim_07_the_live_maximum_warning_shows_in_the_message_line() {
    let mut dump = example_dump();
    dump.set_voltage = Millivolts::new(20_000);
    dump.max_voltage = Millivolts::new(18_400);
    let model = model_with(dump, PORT);
    let lines = draw(&model, 80, 24);
    assert!(lines.iter().any(|l| {
        l.contains("Set 20.00 V is above the live maximum 18.40 V. Lower it to turn output ON")
    }));
    let lines = draw(&model, 44, 16);
    assert!(
        lines
            .iter()
            .any(|l| l.contains("V 20.00 above live max 18.40"))
    );
    // Link loss takes the line over (LNK-02).
    let lines = draw(&lost(model), 80, 24);
    assert!(lines.iter().any(|l| l.contains("Output state unknown")));
    assert!(!lines.iter().any(|l| l.contains("above the live maximum")));
}

#[test]
fn ui_02_main_compact_cv() {
    check("main-compact-cv", &model_with(example_dump(), PORT), 44, 16);
}

#[test]
fn ui_02_main_compact_cc() {
    check("main-compact-cc", &model_with(cc_dump(), PORT), 44, 16);
}

#[test]
fn ui_02_main_compact_lost() {
    check(
        "main-compact-lost",
        &lost(model_with(example_dump(), PORT)),
        44,
        16,
    );
}

#[test]
fn ui_02_main_standard_cv() {
    check(
        "main-standard-cv",
        &model_with(example_dump(), PORT),
        80,
        24,
    );
}

#[test]
fn ui_02_main_standard_cc() {
    check("main-standard-cc", &model_with(cc_dump(), PORT), 80, 24);
}

#[test]
fn ui_02_main_standard_lost() {
    check(
        "main-standard-lost",
        &lost(model_with(example_dump(), PORT)),
        80,
        24,
    );
}

#[test]
fn ui_02_main_large_cv() {
    check("main-large-cv", &model_with(example_dump(), PORT), 100, 21);
}

#[test]
fn ui_02_main_large_cc() {
    check("main-large-cc", &model_with(cc_dump(), PORT), 100, 21);
}

#[test]
fn ui_02_main_cv_120x40() {
    check("main-cv-120x40", &model_with(example_dump(), PORT), 120, 40);
}

#[test]
fn ui_02_main_cc_120x40() {
    check("main-cc-120x40", &model_with(cc_dump(), PORT), 120, 40);
}

#[test]
fn ui_02_toosmall_43x16() {
    let mut dump = example_dump();
    dump.set_voltage = Millivolts::new(3300);
    dump.set_current = Milliamps::new(200);
    check("toosmall-43x16", &model_with(dump, PORT), 43, 16);
}

// ---- invariants over a size sweep (UI-14) ------------------------------------

fn text_of(buf: &Buffer) -> Vec<String> {
    rows(buf)
}

#[test]
fn ui_14_every_size_renders_without_panic_and_keeps_the_essentials_visible() {
    let online = model_with(example_dump(), PORT);
    let down = lost(model_with(example_dump(), PORT));
    // The full range 44×16 .. 250×80, with every column and row near the
    // thresholds and a step elsewhere.
    let cols: Vec<u16> = (44..=250).filter(|c| *c < 110 || c % 7 == 0).collect();
    let row_list: Vec<u16> = (16..=80).filter(|r| *r < 34 || r % 5 == 0).collect();
    for model in [&online, &down] {
        for &cols_ in &cols {
            for &rows_ in &row_list {
                let mut buf = Buffer::empty(Rect::new(0, 0, cols_, rows_));
                render(model, &mut buf, Theme::new(Depth::TrueColor));
                let lines = text_of(&buf);
                let whole = lines.join("\n");
                let at = format!("{cols_}x{rows_}");
                assert!(lines[0].contains("FNIRSI"), "{at}: header");
                assert!(whole.contains("SET"), "{at}: set values");
                assert!(
                    whole.contains("? help") || whole.contains("?  help"),
                    "{at}: help key"
                );
                let status = if model.is_online() {
                    "connected"
                } else {
                    "no link"
                };
                assert!(lines[0].contains(status), "{at}: link status");
                if cols_ >= 80 && rows_ >= 21 {
                    assert_eq!(whole.matches("MEASURED").count(), 2, "{at}");
                    assert!(
                        lines[usize::from(rows_) - 1].contains("help"),
                        "{at}: key bar last"
                    );
                    // Both readout boxes reach the status box without a gap.
                    assert!(whole.contains("╭─ Status"), "{at}: status box");
                    // Fluid fills the width: the first box starts at column 0.
                    assert!(lines[1].starts_with('╭'), "{at}: no left margin");
                    assert_eq!(
                        lines[1].chars().count(),
                        usize::from(cols_),
                        "{at}: full width"
                    );
                }
            }
        }
    }
}

#[test]
fn ui_15_big_digits_step_up_with_the_terminal() {
    let model = model_with(example_dump(), PORT);
    let height_of = |cols, rows_| {
        let lines = draw(&model, cols, rows_);
        // F3 digits are 3 rows between MEASURED and the unit; count rows
        // from MEASURED to the SET label.
        let measured = lines.iter().position(|l| l.contains("MEASURED")).unwrap();
        let set = lines.iter().position(|l| l.contains("SET  ←→")).unwrap();
        set - measured - 2
    };
    assert_eq!(height_of(100, 21), 3);
    assert_eq!(height_of(80, 24), 3);
    assert_eq!(height_of(100, 30), 5);
    assert_eq!(height_of(120, 40), 7);
    assert_eq!(height_of(250, 80), 7, "no font beyond F7");
}

#[test]
fn ui_12_resize_keeps_the_model_and_reevaluates_the_tier() {
    let model = model_with(example_dump(), PORT);
    let big = draw(&model, 120, 40);
    let small = draw(&model, 44, 16);
    let tiny = draw(&model, 30, 10);
    assert!(big.iter().any(|l| l.contains("Memory")));
    assert!(small.iter().any(|l| l.contains("VOLTAGE (V)")));
    assert!(tiny.iter().any(|l| l.contains("Terminal too small")));
    // Back to big: same drawing as before.
    assert_eq!(draw(&model, 120, 40), big);
}

#[test]
fn ui_03_compact_is_centred_and_top_aligned_while_the_bars_span_the_width() {
    let model = model_with(example_dump(), PORT);
    let lines = draw(&model, 60, 20);
    // Body centred: (60 - 44) / 2 = 8 blank cells on the left of the boxes.
    assert!(lines[1].starts_with("        ╭"), "{:?}", lines[1]);
    // Top-aligned: rows below the 14-row block are blank.
    assert!(lines[14..].iter().all(|l| l.is_empty()));
    // The header's bar background reaches both edges.
    let mut buf = Buffer::empty(Rect::new(0, 0, 60, 20));
    render(&model, &mut buf, Theme::new(Depth::TrueColor));
    let bar = buf[(0, 0)].bg;
    assert_eq!(buf[(59, 0)].bg, bar);
    assert_ne!(buf[(0, 1)].bg, bar);
}

#[test]
fn ui_06_only_documented_glyphs_appear_and_each_is_one_cell_wide() {
    let allowed = "╭╮╰╯─│━┏┓┗┛┃┣┫╺╸╻╹▔○●▸◂✓✗!·°→↑↓←⏎…–× ?.-,/:";
    let model = model_with(example_dump(), PORT);
    for (cols_, rows_) in [(44, 16), (80, 24), (100, 21), (120, 40)] {
        for line in draw(&model, cols_, rows_) {
            for ch in line.chars() {
                assert!(
                    ch.is_ascii() || allowed.contains(ch),
                    "{cols_}x{rows_}: unexpected glyph {ch:?} (U+{:04X})",
                    ch as u32
                );
            }
        }
    }
}

// ---- colour (DESIGN.md §3.2, §3.3) -------------------------------------------

fn find(buf: &Buffer, needle: &str) -> (u16, u16) {
    for y in 0..buf.area.height {
        let line: String = (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect();
        if let Some(byte) = line.find(needle) {
            return (line[..byte].chars().count() as u16, y);
        }
    }
    panic!("{needle:?} not on screen");
}

fn rgb(hex: u32) -> ratatui::style::Color {
    ratatui::style::Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

#[test]
fn ui_05_each_quantity_has_its_colour_and_set_digits_are_gold() {
    let model = model_with(example_dump(), PORT);
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 24));
    render(&model, &mut buf, Theme::new(Depth::TrueColor));
    // Voltage digits ch1, current digits ch2, set digits gold, power pw.
    let (x, y) = find(&buf, "VOLTAGE");
    assert_eq!(buf[(x, y)].fg, rgb(0xf6c177), "focused title is gold");
    let measured = find(&buf, "MEASURED");
    assert_eq!(buf[(measured.0 + 1, measured.1 + 1)].fg, rgb(0x4fd1c5));
    let right = find(&buf, "▸ VOLTAGE").0 + 40;
    assert_eq!(buf[(right + 1, measured.1 + 1)].fg, rgb(0x6ea8fe));
    let set = find(&buf, "SET  ←→");
    assert_eq!(buf[(set.0 + 1, set.1 + 1)].fg, rgb(0xf6c177));
    let power = find(&buf, "2.56 W");
    assert_eq!(buf[(power.0, power.1)].fg, rgb(0xb794f6));
    // Header: connected in ok on the bar background; badge ON filled green.
    let connected = find(&buf, "connected");
    assert_eq!(buf[(connected.0, connected.1)].fg, rgb(0x6fd08c));
    assert_eq!(buf[(connected.0, connected.1)].bg, rgb(0x182030));
    let on = find(&buf, " ON ");
    assert_eq!(buf[(on.0 + 1, on.1)].bg, rgb(0x6fd08c));
    // The page is painted explicitly.
    assert_eq!(buf[(0, 10)].bg, rgb(0x0f1420));
}

#[test]
fn ui_05_focus_and_link_loss_are_visible_in_text_not_only_in_colour() {
    // With colour off the focus marker and the badges still tell the state.
    let model = model_with(example_dump(), PORT);
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 24));
    render(&model, &mut buf, Theme::new(Depth::None));
    let lines = rows(&buf);
    assert!(lines.iter().any(|l| l.contains("▸ VOLTAGE")));
    assert!(lines.iter().any(|l| l.contains("OUTPUT  ON")));
    assert!(
        buf.content()
            .iter()
            .all(|c| c.fg == ratatui::style::Color::Reset && c.bg == ratatui::style::Color::Reset)
    );
    let down = lost(model);
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 24));
    render(&down, &mut buf, Theme::new(Depth::None));
    let lines = rows(&buf);
    assert!(lines.iter().any(|l| l.contains("○ no link")));
    assert!(lines.iter().any(|l| l.contains("OUTPUT  ?")));
    assert!(
        lines
            .iter()
            .any(|l| l.contains("Output state unknown, may still be ON"))
    );
}

// ---- secondary screens: invariants and states ------------------------------------

fn bottom_line(lines: &[String], cols_: u16, rows_: u16) -> &str {
    // Compact keeps its bar on row 16 of the block; Fluid uses the last row.
    let row = if cols_ >= 80 && rows_ >= 21 {
        usize::from(rows_) - 1
    } else {
        15
    };
    &lines[row]
}

#[test]
fn ui_16_both_screens_render_at_every_size_with_their_frame_intact() {
    let presets = presets_screen();
    let protection = protection_screen();
    let cols: Vec<u16> = (44..=250).filter(|c| *c < 110 || c % 9 == 0).collect();
    let row_list: Vec<u16> = (16..=80).filter(|r| *r < 34 || r % 6 == 0).collect();
    for (model, name) in [(&presets, "PRESETS"), (&protection, "PROTECTION")] {
        for &cols_ in &cols {
            for &rows_ in &row_list {
                let lines = draw(model, cols_, rows_);
                let at = format!("{name} {cols_}x{rows_}");
                assert!(
                    lines[0].contains("FNIRSI") && lines[0].contains("connected"),
                    "{at}"
                );
                assert!(lines[1].contains(name), "{at}: screen name in the strip");
                assert!(lines[1].contains("4.998 V"), "{at}: live strip");
                let bar = bottom_line(&lines, cols_, rows_);
                assert!(bar.contains("Esc"), "{at}: key bar `{bar}`");
                if cols_ >= 80 && rows_ >= 21 {
                    assert!(
                        lines[2].starts_with('╭'),
                        "{at}: body starts under the strip"
                    );
                    assert_eq!(
                        lines[2].chars().count(),
                        usize::from(cols_),
                        "{at}: full width"
                    );
                }
            }
        }
    }
}

#[test]
fn ui_16_the_edit_box_uses_the_largest_font_that_fits() {
    let height_of = |model: &Model, cols_, rows_, marker: &str| {
        let lines = draw(model, cols_, rows_);
        let first = lines.iter().position(|l| l.contains(marker)).unwrap();
        let second = lines
            .iter()
            .position(|l| l.contains("CURRENT") && !l.contains("╭"))
            .unwrap();
        second - first - 2
    };
    let presets = presets_screen();
    assert_eq!(height_of(&presets, 80, 24, "VOLTAGE"), 5, "F5 at 80x24");
    assert_eq!(height_of(&presets, 120, 40, "VOLTAGE"), 7, "F7 at 120x40");
    assert_eq!(
        height_of(&presets, 80, 21, "VOLTAGE"),
        3,
        "F3 when F5 does not fit"
    );
}

#[test]
fn ui_05_a_prompt_replaces_the_key_bar_with_its_question_and_keys() {
    let mut model = presets_screen();
    model.on_key(Key::Char('s'));
    let lines = draw(&model, 80, 24);
    let bar = lines[23].clone();
    assert!(
        bar.contains("Overwrite M3?  9.00 V/2.000 A → 5.00 V/1.500 A"),
        "{bar}"
    );
    assert!(bar.contains(" y  yes") && bar.contains(" Esc  no"), "{bar}");
    let lines = draw(&model, 44, 16);
    assert!(
        lines[15].contains("Overwrite M3?") && !lines[15].contains("9.00"),
        "{}",
        lines[15]
    );
    // Answering puts the key bar back.
    model.on_key(Key::Char('n'));
    assert!(draw(&model, 80, 24)[23].contains("recall"));
}

#[test]
fn ui_05_a_message_replaces_the_key_bar_and_the_next_key_restores_it() {
    let mut dump = example_dump();
    dump.output_on = false;
    let mut model = named(model_with(dump, PORT));
    for key in [Key::Char('p'), Key::Down, Key::Down, Key::Enter] {
        model.on_key(key);
    }
    assert!(draw(&model, 80, 24)[23].contains("Recalled M3 · 9.00 V / 2.000 A"));
    model.on_key(Key::Up);
    assert!(draw(&model, 80, 24)[23].contains("recall"));
}

#[test]
fn ui_05_the_chord_message_shows_on_the_main_panel() {
    let mut dump = example_dump();
    dump.output_on = false;
    let mut model = model_with(dump, PORT);
    model.on_key(Key::Char('m'));
    assert!(
        draw(&model, 80, 24)
            .iter()
            .any(|l| l.contains("Recall which preset? 1-6, Esc cancels"))
    );
    assert!(
        draw(&model, 44, 16)
            .iter()
            .any(|l| l.contains("Recall which? 1-6, Esc"))
    );
}

#[test]
fn lnk_02_the_strip_shows_dashes_and_an_unknown_output_while_the_link_is_down() {
    let down = lost(presets_screen());
    let lines = draw(&down, 80, 24);
    assert!(lines[0].contains("no link"));
    assert!(
        lines[1].contains("-.--- V") && lines[1].contains("-.--- A") && lines[1].contains("-.-- W")
    );
    assert!(lines[1].contains(" ?"), "{}", lines[1]);
    assert!(!lines[1].contains("4.998"));
    let lines = draw(&down, 44, 16);
    assert!(lines[1].contains("-.--- V") && lines[1].contains(" ?"));
}

#[test]
fn ui_16_the_strip_shows_off_and_the_mode_when_on() {
    let mut dump = example_dump();
    dump.output_on = false;
    let off = draw(&named(model_with(dump, PORT)), 80, 24);
    let mut model = named(model_with(example_dump(), PORT));
    model.on_key(Key::Char('p'));
    let on = draw(&model, 80, 24);
    assert!(on[1].contains("CV · ON"));
    let mut cc = named(model_with(cc_dump(), PORT));
    cc.on_key(Key::Char('p'));
    assert!(draw(&cc, 80, 24)[1].contains("CC · ON"));
    assert!(draw(&cc, 44, 16)[1].contains(" ON"));
    let mut dump = example_dump();
    dump.output_on = false;
    let mut model = named(model_with(dump, PORT));
    model.on_key(Key::Char('p'));
    assert!(draw(&model, 80, 24)[1].contains(" OFF"));
    let _ = off;
}

#[test]
fn lim_06_a_stored_violation_is_shown_on_the_protection_screen_and_not_hidden() {
    let mut dump = example_dump();
    dump.ovp = Millivolts::new(5000); // not above the 5.00 V setpoint
    let mut model = named(model_with(dump, PORT));
    model.on_key(Key::Char('o'));
    let lines = draw(&model, 80, 24);
    assert!(
        lines
            .iter()
            .any(|l| l.contains("✗ Set voltage 5.00 V is not below OVP 5.00 V"))
    );
    assert!(!lines.iter().any(|l| l.contains("no protection tripped")));
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 24));
    render(&model, &mut buf, Theme::new(Depth::TrueColor));
    let (x, y) = find(&buf, "5.00 V   0 – 30 V");
    assert_eq!(
        buf[(x, y)].fg,
        rgb(0xff5c7a),
        "the offending limit is drawn in er"
    );
}

#[test]
fn lim_08_a_tripped_protection_is_named_on_the_protection_screen() {
    let mut dump = example_dump();
    dump.protection = 4;
    dump.output_on = false;
    let mut model = named(model_with(dump, PORT));
    model.on_key(Key::Char('o'));
    let lines = draw(&model, 80, 24);
    assert!(
        lines
            .iter()
            .any(|l| l.contains("● OTP tripped   last trip: OTP"))
    );
}

#[test]
fn ui_05_the_selected_row_is_highlighted_to_the_edge_of_the_box_interior() {
    let model = presets_screen();
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 24));
    render(&model, &mut buf, Theme::new(Depth::TrueColor));
    let (x, y) = find(&buf, "▸ M3");
    let sel = rgb(0x2d3b5e);
    assert_eq!(buf[(x, y)].bg, sel);
    // From the first inner column to the last, and not on the border.
    assert_eq!(buf[(2, y)].bg, sel);
    assert_eq!(buf[(45, y)].bg, sel);
    assert_ne!(buf[(46, y)].bg, sel);
    assert_ne!(buf[(0, y)].bg, sel);
    // Another row is not highlighted.
    assert_ne!(buf[(5, y - 1)].bg, sel);
    // With colour off the selection is a reverse-video row.
    let mut buf = Buffer::empty(Rect::new(0, 0, 80, 24));
    render(&model, &mut buf, Theme::new(Depth::None));
    assert!(
        buf[(x, y)]
            .modifier
            .contains(ratatui::style::Modifier::REVERSED)
    );
}

#[test]
fn pre_04_renaming_shows_the_field_in_the_box_title_and_a_matching_key_bar() {
    let mut model = presets_screen();
    model.on_key(Key::Char('n'));
    let lines = draw(&model, 80, 24);
    assert!(
        lines.iter().any(|l| l.contains("M3 · 9V pedal")),
        "title holds the field"
    );
    let bar = &lines[23];
    assert!(
        bar.contains("⏎  save") && bar.contains("Esc  cancel") && !bar.contains("help"),
        "{bar}"
    );
}

#[test]
fn lim_05_the_protection_edit_box_flags_an_invalid_candidate_and_a_maximum_default() {
    let mut model = named(model_with(example_dump(), PORT));
    for key in [
        Key::Char('o'),
        Key::Down,
        Key::Enter,
        Key::Left,
        Key::Down,
        Key::Down,
        Key::Down,
        Key::Down,
    ] {
        model.on_key(key);
    }
    // OCP candidate is now 1.10 A, below the 1.500 A current limit.
    let lines = draw(&model, 80, 24);
    assert!(
        lines.iter().any(|l| l.contains("✗ OCP not above it")),
        "{lines:#?}"
    );
    // `r` loads the default, which equals the device maximum for OCP.
    model.on_key(Key::Char('r'));
    let lines = draw(&model, 80, 24);
    assert!(
        lines
            .iter()
            .any(|l| l.contains("! default is the device maximum: no protection"))
    );
    assert!(lines.iter().any(|l| l.contains("✓ OCP stays above it")));
}
