//! The main panel at every tier (DESIGN.md §2, §5.5 to §5.10, §6).

use app::{Focus, LinkStatus, Model};
use protocol::Mode;
use protocol::value::{Milliamps, Millivolts};
use ratatui::layout::Rect;

use crate::font::Font;
use crate::format;
use crate::layout::{COMPACT_ROWS, COMPACT_WIDTH, Tier, pick_font};
use crate::paint::Painter;
use crate::theme::S;

const LAMPS: [&str; 5] = ["OVP", "OCP", "OPP", "OTP", "LVP"];
const MAX_POWER: f32 = 150.0;

/// Output state as the badges show it (§5.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Output {
    On,
    Off,
    Unknown,
}

/// Which mode badge is lit (§6.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lit {
    Cv,
    Cc,
    Neither,
}

/// What the screen needs, taken from the model once per frame.
struct View {
    online: bool,
    auto_reconnect: bool,
    output: Output,
    lit: Lit,
    /// `DC`: 0 none, 1 OVP .. 5 LVP.
    protection: u8,
    set_voltage: Option<Millivolts>,
    set_current: Option<Milliamps>,
    /// Live readings; `None` while the link is down or before any data.
    measured: Option<(f32, f32, f32)>,
    vin: Option<f32>,
    temperature: Option<f32>,
    capacity: Option<f32>,
    energy: Option<f32>,
    live_max_voltage: Option<Millivolts>,
    live_max_current: Option<Milliamps>,
    presets: Option<[(Millivolts, Milliamps); 6]>,
    model_name: String,
    firmware: Option<String>,
    hardware: Option<String>,
    port: String,
    notice: Option<String>,
    focus: Focus,
    cursors: [usize; 2],
}

impl View {
    fn new(model: &Model) -> Self {
        let online = model.is_online();
        let auto_reconnect = matches!(
            model.link,
            LinkStatus::Lost {
                auto_reconnect: true
            }
        );
        let snapshot = model.snapshot.as_ref();
        let dump = snapshot.map(|s| &s.dump);
        let protection = dump.map_or(0, |d| d.protection);
        let output_on = dump.is_some_and(|d| d.output_on);
        let output = if !online {
            Output::Unknown
        } else if output_on {
            Output::On
        } else {
            Output::Off
        };
        let lit = match (online, output_on, protection, dump.map(|d| d.mode)) {
            (true, true, 0, Some(Mode::ConstantVoltage)) => Lit::Cv,
            (true, true, 0, Some(Mode::ConstantCurrent)) => Lit::Cc,
            _ => Lit::Neither,
        };
        let live = |value: Option<f32>| value.filter(|_| online);
        Self {
            online,
            auto_reconnect,
            output,
            lit,
            protection,
            set_voltage: dump.map(|d| d.set_voltage),
            set_current: dump.map(|d| d.set_current),
            measured: dump
                .filter(|_| online)
                .map(|d| (d.output.voltage, d.output.current, d.output.power)),
            vin: live(dump.map(|d| d.input_voltage)),
            temperature: live(dump.map(|d| d.temperature)),
            capacity: live(dump.map(|d| d.capacity_ah)),
            energy: live(dump.map(|d| d.energy_wh)),
            live_max_voltage: dump.map(|d| d.max_voltage),
            live_max_current: dump.map(|d| d.max_current),
            presets: dump
                .map(|d| std::array::from_fn(|i| (d.presets[i].voltage, d.presets[i].current))),
            model_name: snapshot
                .and_then(|s| s.model.clone())
                .unwrap_or_else(|| "DPS-150".into()),
            firmware: snapshot.and_then(|s| s.firmware.clone()),
            hardware: snapshot.and_then(|s| s.hardware.clone()),
            port: model.port.clone(),
            notice: model.notice.clone(),
            focus: model.focus,
            cursors: [model.cursor(Focus::Voltage), model.cursor(Focus::Current)],
        }
    }

    fn tripped(&self) -> bool {
        self.online && self.protection != 0
    }
}

/// Draws the main panel into `area` of `p`'s buffer.
pub(crate) fn draw(p: &mut Painter, model: &Model, area: Rect, tier: Tier) {
    let view = View::new(model);
    match tier {
        Tier::Compact => compact(p, &view, area),
        Tier::Standard | Tier::Large => fluid(p, &view, area, tier),
        Tier::TooSmall => unreachable!("handled by the caller"),
    }
}

/// The port name for the Standard header: the file name without the macOS
/// `cu.` / `tty.` class prefix (`usbmodem13C85FCE40251`).
fn short_port(path: &str) -> String {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.strip_prefix("cu.")
        .or_else(|| name.strip_prefix("tty."))
        .unwrap_or(name)
        .to_string()
}

// ---- header ----------------------------------------------------------------

fn header(p: &mut Painter, view: &View, x: u16, y: u16, width: u16, tier: Tier) {
    p.fill(Rect::new(x, y, width, 1), S::Hd);
    let (label, label_style) = if !view.online {
        (" ○ no link ", S::Her)
    } else if view.tripped() {
        (" ● PROTECTION TRIPPED ", S::Her)
    } else {
        (" ● connected ", S::Hok)
    };
    let mut model_text = format!(" {}", view.model_name);
    let mut port_text = String::new();
    match tier {
        Tier::Standard | Tier::Large => {
            if let Some(fw) = &view.firmware {
                model_text.push_str(&format!(" · fw {fw}"));
            }
            if tier == Tier::Large
                && let Some(hw) = &view.hardware
            {
                model_text.push_str(&format!(" · hw {hw}"));
            }
            let path = if tier == Tier::Large {
                view.port.clone()
            } else {
                short_port(&view.port)
            };
            port_text = format!(" {path} ");
        }
        _ => {}
    }
    let fits = |model_text: &str, port_text: &str| {
        " FNIRSI ".chars().count()
            + model_text.chars().count()
            + label.chars().count()
            + port_text.chars().count()
            <= usize::from(width)
    };
    // Drop the port first, then the firmware and hardware text (§5.9).
    if !fits(&model_text, &port_text) {
        port_text.clear();
    }
    if !fits(&model_text, &port_text) {
        model_text = format!(" {}", view.model_name);
    }
    let right_width = (label.chars().count() + port_text.chars().count()) as u16;
    p.spans(x, y, &[(" FNIRSI ", S::Hb), (&model_text, S::Hd)]);
    let right_x = x + width.saturating_sub(right_width);
    p.spans(right_x, y, &[(label, label_style), (&port_text, S::Hd)]);
}

// ---- key bar ---------------------------------------------------------------

const KEYS: [(&str, &str); 9] = [
    ("Space", "output"),
    ("←→", "digit"),
    ("↑↓", "value"),
    ("⏎", "type"),
    ("Tab", "V/I"),
    ("p", "presets"),
    ("o", "protect"),
    (",", "settings"),
    ("m", "recall"),
];
const TAIL: (&str, &str) = ("?", "help");

fn item_width(key: &str, label: &str) -> usize {
    key.chars().count() + label.chars().count() + 5
}

/// The key bar algorithm of DESIGN.md §5.8 for layout width `layout_width`.
fn key_bar_items(layout_width: usize) -> Vec<(&'static str, &'static str)> {
    let mut room = layout_width.saturating_sub(item_width(TAIL.0, TAIL.1));
    let mut items = Vec::new();
    for (key, label) in KEYS {
        let width = item_width(key, label);
        if width > room {
            break;
        }
        room -= width;
        items.push((key, label));
    }
    items.push(TAIL);
    items
}

fn key_bar(p: &mut Painter, x: u16, y: u16, layout_width: u16) {
    let mut column = x;
    for (key, label) in key_bar_items(usize::from(layout_width)) {
        let cap = format!(" {key} ");
        let label = format!(" {label}  ");
        column = p.spans(column, y, &[(&cap, S::K), (&label, S::D)]);
    }
}

// ---- pieces shared by the tiers ----------------------------------------------

fn badge(p: &mut Painter, x: u16, y: u16, text: &str, s: S) -> u16 {
    p.text(x, y, &format!(" {text} "), s)
}

fn mode_badges_fluid(p: &mut Painter, x: u16, y: u16, view: &View) -> u16 {
    let cv = if view.lit == Lit::Cv { S::Bcv } else { S::Boff };
    let cc = if view.lit == Lit::Cc { S::Bcc } else { S::Boff };
    let column = badge(p, x, y, "CV", cv);
    let column = p.text(column, y, " ", S::Page);
    badge(p, column, y, "CC", cc)
}

fn output_badge(p: &mut Painter, x: u16, y: u16, view: &View) -> u16 {
    match view.output {
        Output::On => badge(p, x, y, "ON", S::Bon),
        Output::Off => badge(p, x, y, "OFF", S::Boff),
        Output::Unknown => badge(p, x, y, "?", S::Bun),
    }
}

fn lamps(p: &mut Painter, x: u16, y: u16, view: &View) -> u16 {
    let mut column = x;
    for (index, name) in LAMPS.iter().enumerate() {
        if index > 0 {
            column = p.text(column, y, "  ", S::Page);
        }
        let (glyph, s) = if !view.online {
            ("?", S::D)
        } else if usize::from(view.protection) == index + 1 {
            ("●", S::Er)
        } else {
            ("○", S::D)
        };
        column = p.spans(column, y, &[(glyph, s), (" ", S::Page), (name, s)]);
    }
    column
}

fn power_text(view: &View) -> String {
    view.measured.map_or_else(
        || "-.-- W".to_string(),
        |(_, _, power)| format::power(power),
    )
}

/// The link-lost sentence for the message line (§6.7).
fn lost_spans(view: &View, compact: bool) -> Vec<(String, S)> {
    if compact {
        let tail = if view.auto_reconnect {
            "retrying 1 s"
        } else {
            "r retry"
        };
        vec![
            ("Output unknown".into(), S::Er),
            (format!(" · {tail}"), S::D),
        ]
    } else {
        let tail = if view.auto_reconnect {
            "  retrying every 1 s"
        } else {
            "  reconnect off · r retry"
        };
        vec![
            ("Output state unknown, may still be ON".into(), S::Er),
            (tail.into(), S::D),
        ]
    }
}

fn draw_spans(p: &mut Painter, x: u16, y: u16, spans: &[(String, S)]) -> u16 {
    spans
        .iter()
        .fold(x, |column, (text, s)| p.text(column, y, text, *s))
}

// ---- Compact -----------------------------------------------------------------

fn compact(p: &mut Painter, view: &View, area: Rect) {
    let x0 = area.x + area.width.saturating_sub(COMPACT_WIDTH) / 2;
    let y0 = area.y;
    header(p, view, x0, y0, COMPACT_WIDTH, Tier::Compact);
    // The header's fill reaches both edges of the terminal.
    p.fill(Rect::new(area.x, y0, x0 - area.x, 1), S::Hd);
    p.fill(
        Rect::new(
            x0 + COMPACT_WIDTH,
            y0,
            (area.x + area.width).saturating_sub(x0 + COMPACT_WIDTH),
            1,
        ),
        S::Hd,
    );

    compact_readout(p, view, Rect::new(x0, y0 + 1, 22, 7), Focus::Voltage);
    compact_readout(p, view, Rect::new(x0 + 22, y0 + 1, 22, 7), Focus::Current);

    // Status box.
    let status = Rect::new(x0, y0 + 8, COMPACT_WIDTH, 4);
    p.boxed(status, ("Status", S::Ti), None, S::Bd);
    let inner_x = status.x + 2;
    let mode = match view.lit {
        Lit::Cv => (" CV ", S::Bcv),
        Lit::Cc => (" CC ", S::Bcc),
        Lit::Neither => (" -- ", S::Boff),
    };
    let column = p.text(inner_x, status.y + 1, mode.0, mode.1);
    let column = p.text(column, status.y + 1, " ", S::Page);
    // The unknown badge takes the 5 cells of ` OFF ` so the power value keeps
    // its column (main-compact-lost).
    let column = match view.output {
        Output::Unknown => p.text(column, status.y + 1, "  ?  ", S::Bun),
        _ => output_badge(p, column, status.y + 1, view),
    };
    let column = p.text(column, status.y + 1, "  ", S::Page);
    p.text(column, status.y + 1, &power_text(view), S::Pw);
    lamps(p, inner_x, status.y + 2, view);

    // Info row.
    let info_y = y0 + 12;
    if !view.online {
        draw_spans(p, x0 + 1, info_y, &lost_spans(view, true));
    } else if let Some(notice) = &view.notice {
        p.text(x0 + 1, info_y, notice, S::Wn);
    } else if let (Some(vin), Some(temp), Some(energy)) = (view.vin, view.temperature, view.energy)
    {
        p.spans(
            x0 + 1,
            info_y,
            &[
                ("Vin ", S::D),
                (&format::input_voltage(vin), S::Tx),
                (" · ", S::D),
                (&format::temperature(temp), S::Tx),
                (" · ", S::D),
                (&format::energy(energy), S::Tx),
            ],
        );
    }
    debug_assert_eq!(y0 + COMPACT_ROWS - 1, y0 + 13);
    key_bar(p, x0, y0 + 13, COMPACT_WIDTH);
}

fn compact_readout(p: &mut Painter, view: &View, rect: Rect, which: Focus) {
    let focused = view.focus == which;
    let name = match which {
        Focus::Voltage => "VOLTAGE (V)",
        Focus::Current => "CURRENT (A)",
    };
    let (title, title_style, border) = if focused {
        (format!("▸ {name}"), S::Set, S::Bdf)
    } else {
        (name.to_string(), S::Ti, S::Bd)
    };
    p.boxed(rect, (&title, title_style), None, border);
    let (x, y) = (rect.x + 2, rect.y + 1);
    let (digits, digit_style, set_text) = match which {
        Focus::Voltage => (
            view.measured.map_or_else(
                || "-.---".to_string(),
                |(v, _, _)| format::measured_voltage(v, true),
            ),
            S::Ch1,
            view.set_voltage
                .map_or_else(|| "--.--".to_string(), format::set_voltage),
        ),
        Focus::Current => (
            view.measured.map_or_else(
                || "-.---".to_string(),
                |(_, i, _)| format::measured_current(i),
            ),
            S::Ch2,
            view.set_current
                .map_or_else(|| "-.---".to_string(), format::set_current),
        ),
    };
    for (row, line) in crate::font::F3.render(&digits).iter().enumerate() {
        p.text(x, y + row as u16, line, digit_style);
    }
    let column = p.spans(x, y + 3, &[("SET ", S::D), (&set_text, S::Set)]);
    if which == Focus::Current && view.lit == Lit::Cc {
        p.text(column, y + 3, " ◂ LIM", S::Wn);
    }
    if focused {
        let index = char_index(which, view.cursors[which_index(which)]);
        p.text(x + 4 + index as u16, y + 4, "▔", S::Cur);
    }
}

// ---- Fluid -------------------------------------------------------------------

fn fluid(p: &mut Painter, view: &View, area: Rect, tier: Tier) {
    let (cols, rows) = (area.width, area.height);
    let large = tier == Tier::Large;
    header(p, view, area.x, area.y, cols, tier);
    let body_rows = rows - if large { 9 } else { 7 };
    let left_width = cols - cols / 2;
    let right_width = cols / 2;
    let readouts_y = area.y + 1;

    let inner_height = usize::from(body_rows) - 2;
    let font = pick_font(usize::from(right_width) - 4, inner_height);
    fluid_readout(
        p,
        view,
        Rect::new(area.x, readouts_y, left_width, body_rows),
        Focus::Voltage,
        font,
    );
    fluid_readout(
        p,
        view,
        Rect::new(area.x + left_width, readouts_y, right_width, body_rows),
        Focus::Current,
        font,
    );

    let status = Rect::new(area.x, readouts_y + body_rows, cols, 4);
    fluid_status(p, view, status, tier);

    let below = status.y + 4;
    if large {
        memory_box(p, view, Rect::new(area.x, below, cols, 3));
    } else {
        memory_row(p, view, area.x, below, cols);
    }
    key_bar(p, area.x, area.y + rows - 1, cols);
}

fn which_index(which: Focus) -> usize {
    match which {
        Focus::Voltage => 0,
        Focus::Current => 1,
    }
}

/// Character position of digit `index` (digits only) in the set text.
fn char_index(which: Focus, index: usize) -> usize {
    match which {
        // DD.DD
        Focus::Voltage => [0, 1, 3, 4][index.min(3)],
        // D.DDD
        Focus::Current => [0, 2, 3, 4][index.min(3)],
    }
}

fn fluid_readout(p: &mut Painter, view: &View, rect: Rect, which: Focus, font: Font) {
    let focused = view.focus == which;
    let name = match which {
        Focus::Voltage => "VOLTAGE",
        Focus::Current => "CURRENT",
    };
    let (title, title_style, border) = if focused {
        (format!("▸ {name}"), S::Set, S::Bdf)
    } else {
        (name.to_string(), S::Ti, S::Bd)
    };
    p.boxed(rect, (&title, title_style), None, border);

    let h = font.height() as u16;
    let inner = Rect::new(rect.x + 2, rect.y + 1, rect.width - 4, rect.height - 2);
    let block = 2 * h + 4;
    let spare = inner.height.saturating_sub(block);
    let top = inner.y + spare / 2;
    let margin = inner.x + 1;

    let (digits, digit_style, unit, set_text) = match which {
        Focus::Voltage => (
            view.measured.map_or_else(
                || "-.---".to_string(),
                |(v, _, _)| format::measured_voltage(v, false),
            ),
            S::Ch1,
            "V",
            view.set_voltage
                .map_or_else(|| "--.--".to_string(), format::set_voltage),
        ),
        Focus::Current => (
            view.measured.map_or_else(
                || "-.---".to_string(),
                |(_, i, _)| format::measured_current(i),
            ),
            S::Ch2,
            "A",
            view.set_current
                .map_or_else(|| "-.---".to_string(), format::set_current),
        ),
    };

    p.text(inner.x, top, "MEASURED", S::D);
    let rows = font.render(&digits);
    for (row, line) in rows.iter().enumerate() {
        let y = top + 1 + row as u16;
        let end = p.text(margin, y, line, digit_style);
        if row + 1 == rows.len() {
            let end = p.text(end, y, &format!("  {unit}"), digit_style);
            if which == Focus::Current && view.lit == Lit::Cc {
                p.text(end, y, "   ◂ limiting", S::Wn);
            }
        }
    }
    let set_y = top + 1 + h + 1;
    p.text(inner.x, set_y, "SET  ←→ digit  ↑↓ value", S::D);
    for (row, line) in font.render(&set_text).iter().enumerate() {
        p.text(margin, set_y + 1 + row as u16, line, S::Set);
    }
    if focused {
        let index = char_index(which, view.cursors[which_index(which)]);
        let x = margin + font.start_of(&set_text, index) as u16;
        let glyph = "▔".repeat(font.height());
        p.text(x, set_y + 1 + h, &glyph, S::Cur);
    }
}

fn fluid_status(p: &mut Painter, view: &View, rect: Rect, tier: Tier) {
    p.boxed(rect, ("Status", S::Ti), None, S::Bd);
    let large = tier == Tier::Large;
    let x = rect.x + 2;
    let (y1, y2) = (rect.y + 1, rect.y + 2);

    // Line 1.
    let column = p.text(x, y1, "MODE ", S::D);
    let column = mode_badges_fluid(p, column, y1, view);
    let column = p.text(column, y1, "   OUTPUT ", S::D);
    let column = output_badge(p, column, y1, view);
    let column = p.text(column, y1, "   PROTECT ", S::D);
    let column = lamps(p, column, y1, view);
    if large && view.online && view.protection == 0 {
        p.spans(column, y1, &[("    ", S::Page), ("all clear", S::Ok)]);
    }

    // Line 2.
    if !view.online {
        draw_spans(p, x, y2, &lost_spans(view, false));
        return;
    }
    if let Some(notice) = &view.notice {
        p.text(x, y2, notice, S::Wn);
        return;
    }
    let extra = usize::from(rect.width).saturating_sub(if large { 100 } else { 80 });
    let bar_width = if large { 16 } else { 18 } + extra;
    let power = view.measured.map_or(0.0, |(_, _, p)| p);
    let column = p.spans(
        x,
        y2,
        &[("POWER ", S::D), (&power_text(view), S::Pw), (" ", S::Page)],
    );
    let column = p.bar(column, y2, power / MAX_POWER, bar_width, S::Pw);
    let (vin, temp) = (
        view.vin
            .map_or_else(|| "--.-- V".to_string(), format::input_voltage),
        view.temperature
            .map_or_else(|| "--.- °C".to_string(), format::temperature),
    );
    if large {
        let capacity = view
            .capacity
            .map_or_else(|| "-.--- Ah".to_string(), format::capacity);
        let energy = view
            .energy
            .map_or_else(|| "-.--- Wh".to_string(), format::energy);
        p.spans(
            column,
            y2,
            &[
                (" of 150 W   Vin ", S::D),
                (&vin, S::Tx),
                ("   Temp ", S::D),
                (&temp, S::Tx),
                ("   Q ", S::D),
                (&capacity, S::Tx),
                ("   E ", S::D),
                (&energy, S::Tx),
            ],
        );
    } else {
        p.spans(
            column,
            y2,
            &[
                (" of 150 W   Vin ", S::D),
                (&vin, S::Tx),
                ("  T ", S::D),
                (&temp, S::Tx),
            ],
        );
    }
}

/// Is preset `index` outside the live maximum (§6.5)?
fn blocked(view: &View, voltage: Millivolts, current: Milliamps) -> bool {
    view.live_max_voltage.is_some_and(|max| voltage > max)
        || view.live_max_current.is_some_and(|max| current > max)
}

fn matches_setpoint(view: &View, voltage: Millivolts, current: Milliamps) -> bool {
    view.set_voltage == Some(voltage) && view.set_current == Some(current)
}

const HINT: &str = "m 1-6 recall";

/// Standard band: one row of preset buttons (§6.5).
fn memory_row(p: &mut Painter, view: &View, x: u16, y: u16, cols: u16) {
    let Some(presets) = view.presets else { return };
    let mut column = x;
    for (index, (voltage, current)) in presets.iter().enumerate() {
        let blocked = blocked(view, *voltage, *current);
        let cap = if blocked {
            format!("!M{} ", index + 1)
        } else {
            format!(" M{} ", index + 1)
        };
        let text = format!(" {} ", format::preset_short(*voltage, *current));
        let (cap_style, text_style) = if blocked {
            (S::Wn, S::Wn)
        } else if matches_setpoint(view, *voltage, *current) {
            (S::K, S::Sel)
        } else {
            (S::K, S::D)
        };
        column = p.spans(
            column,
            y,
            &[(&cap, cap_style), (&text, text_style), (" ", S::Page)],
        );
    }
    // One cell is left blank after the hint, as after a key bar item.
    let hint_x = x + cols - HINT.chars().count() as u16 - 1;
    p.text(hint_x, y, HINT, S::D);
}

/// Large band: the `Memory` box (§6.5).
fn memory_box(p: &mut Painter, view: &View, rect: Rect) {
    p.boxed(rect, ("Memory", S::Ti), Some((HINT, S::D)), S::Bd);
    let Some(presets) = view.presets else { return };
    let mut column = rect.x + 2;
    let y = rect.y + 1;
    for (index, (voltage, current)) in presets.iter().enumerate() {
        if index > 0 {
            column = p.text(column, y, "  ", S::Page);
        }
        let blocked = blocked(view, *voltage, *current);
        let cap = if blocked {
            format!("!M{} ", index + 1)
        } else {
            format!(" M{} ", index + 1)
        };
        let text = format!(" {}", format::preset_long(*voltage, *current));
        let (cap_style, text_style) = if blocked {
            (S::Wn, S::Wn)
        } else if matches_setpoint(view, *voltage, *current) {
            (S::K, S::Sel)
        } else {
            (S::K, S::D)
        };
        column = p.spans(column, y, &[(&cap, cap_style), (&text, text_style)]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_08_the_standard_header_shows_the_port_without_its_class_prefix() {
        assert_eq!(
            short_port("/dev/cu.usbmodem13C85FCE40251"),
            "usbmodem13C85FCE40251"
        );
        assert_eq!(short_port("/dev/tty.usbmodem1"), "usbmodem1");
        assert_eq!(short_port("/dev/ttyACM0"), "ttyACM0");
        assert_eq!(short_port("COM3"), "COM3");
    }

    fn widths(layout: usize) -> Vec<(&'static str, &'static str)> {
        key_bar_items(layout)
    }

    #[test]
    fn ui_08_the_key_bar_reproduces_the_documented_items_at_each_width() {
        let keys = |layout| widths(layout).iter().map(|(k, _)| *k).collect::<Vec<_>>();
        assert_eq!(keys(44), ["Space", "←→", "?"]);
        assert_eq!(keys(80), ["Space", "←→", "↑↓", "⏎", "Tab", "?"]);
        assert_eq!(keys(100), ["Space", "←→", "↑↓", "⏎", "Tab", "p", "o", "?"]);
        assert_eq!(
            keys(120),
            ["Space", "←→", "↑↓", "⏎", "Tab", "p", "o", ",", "?"]
        );
    }

    #[test]
    fn ui_08_help_is_always_last_and_always_present() {
        for width in [20, 44, 80, 250] {
            assert_eq!(widths(width).last(), Some(&TAIL), "{width}");
        }
    }
}
