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
