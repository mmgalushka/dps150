//! The Presets and Protection screens and their shared frame (DESIGN.md
//! §5.10, §8.1, §8.3, §8.5).

use app::protection::{self, ALL, Limit};
use app::{Focus, Model, PresetMode, ProtectionMode, Screen};
use ratatui::layout::Rect;

use crate::font::{self, Font};
use crate::format;
use crate::layout::{COMPACT_WIDTH, Tier};
use crate::main_panel::{
    Lit, Output, View, compact_header, draw_key_items, draw_prompt, fit_items, header,
    message_style,
};
use crate::paint::Painter;
use crate::theme::S;

pub(crate) fn draw(p: &mut Painter, model: &Model, area: Rect, tier: Tier) {
    let compact = tier == Tier::Compact;
    let view = View::new(model, compact);
    let (x0, width) = if compact {
        (
            area.x + area.width.saturating_sub(COMPACT_WIDTH) / 2,
            COMPACT_WIDTH,
        )
    } else {
        (area.x, area.width)
    };
    if compact {
        compact_header(p, &view, area);
    } else {
        header(p, &view, area.x, area.y, area.width, tier);
    }
    let name = screen_name(model.screen());
    strip(p, &view, area, x0, width, name);

    let body_y = area.y + 2;
    let (bar_y, body_h) = if compact {
        // Settings is shorter than the 16-row Compact frame (`settings-compact`).
        let bar = if model.screen() == Screen::Settings {
            12
        } else {
            15
        };
        (area.y + bar, 0)
    } else {
        (area.y + area.height - 1, area.height - 3)
    };
    match (model.screen(), compact) {
        (Screen::Settings, false) => {
            crate::settings::fluid(p, model, &view, Rect::new(x0, body_y, width, body_h))
        }
        (Screen::Settings, true) => crate::settings::compact(p, model, x0, body_y),
        (Screen::Presets, false) => {
            presets_fluid(p, model, &view, Rect::new(x0, body_y, width, body_h))
        }
        (Screen::Presets, true) => presets_compact(p, model, &view, x0, body_y),
        (_, false) => protection_fluid(p, model, &view, Rect::new(x0, body_y, width, body_h)),
        (_, true) => protection_compact(p, model, &view, x0, body_y),
    }
    bottom_row(p, model, x0, bar_y, width, compact);
}

pub(crate) fn screen_name(screen: Screen) -> &'static str {
    match screen {
        Screen::Presets => "PRESETS",
        Screen::Settings => "SETTINGS",
        _ => "PROTECTION",
    }
}

// ---- strip (§5.10) --------------------------------------------------------------

pub(crate) fn strip(p: &mut Painter, view: &View, area: Rect, x0: u16, width: u16, name: &str) {
    let y = area.y + 1;
    p.fill(Rect::new(area.x, y, area.width, 1), S::Hd);
    let (v, i, w) = match view.measured {
        Some((v, i, w)) => (
            format::measured_voltage(v, false),
            format::measured_current(i),
            format::power(w).trim_end_matches(" W").to_string(),
        ),
        None => ("-.---".into(), "-.---".into(), "-.--".into()),
    };
    let column = p.spans(
        x0,
        y,
        &[
            (&format!(" {v} V "), S::Hv),
            (&format!(" {i} A "), S::Hi),
            (&format!(" {w} W "), S::Hp),
        ],
    );
    let wide = width >= 60;
    match (view.output, wide) {
        (Output::Unknown, _) => {
            p.text(column, y, " ?", S::Her);
        }
        (Output::On, true) => {
            let mode = if view.lit == Lit::Cc { "CC" } else { "CV" };
            p.spans(
                column,
                y,
                &[(&format!(" {mode} · "), S::Hd), ("ON", S::Hok)],
            );
        }
        (Output::On, false) => {
            p.spans(column, y, &[(" ", S::Hd), ("ON", S::Hok)]);
        }
        (Output::Off, _) => {
            p.text(column, y, " OFF", S::Hd);
        }
    }
    let label = format!(" {name} ");
    let right = x0 + width - label.chars().count() as u16;
    p.text(right, y, &label, S::Hscr);
}

// ---- bottom row: key bar, message or prompt ----------------------------------------

fn bottom_row(p: &mut Painter, model: &Model, x0: u16, y: u16, width: u16, compact: bool) {
    // A prompt or message replaces the key bar (§5.12).
    if let Some(text) = model.prompt_text(compact) {
        draw_prompt(p, x0 + 1, y, &text, &model.prompt_keys(compact));
        return;
    }
    if let Some(line) = model.message(compact) {
        p.text(x0, y, &format!(" {}", line.text), message_style(line.tone));
        return;
    }
    let (items, tail) = key_lists(model, compact);
    let fitted = fit_items(usize::from(width), &items, &tail);
    draw_key_items(p, x0, y, &fitted);
}

type Keys = Vec<(&'static str, &'static str)>;

/// The priority list and the reserved tail of the current state (§8.1, §8.3).
fn key_lists(model: &Model, compact: bool) -> (Keys, Keys) {
    let help = ("?", "help");
    match model.screen() {
        Screen::Settings if compact => (
            vec![("←→", "change"), ("↑↓", "item")],
            vec![("Esc", "back"), help],
        ),
        Screen::Settings => (
            vec![
                ("↑↓", "item"),
                ("←→", "change"),
                ("Tab", "next group"),
                ("Space", "output"),
            ],
            vec![("Esc", "back"), help],
        ),
        Screen::Presets => match &model.presets_state().mode {
            PresetMode::Rename(_) => (vec![("⏎", "save")], vec![("Esc", "cancel")]),
            PresetMode::Edit(_) => (
                vec![
                    ("←→", "digit"),
                    ("↑↓", "value"),
                    ("Tab", "V/I"),
                    ("⏎", "write"),
                ],
                vec![("Esc", "cancel"), help],
            ),
            PresetMode::List if compact => (
                vec![
                    ("⏎", "recall"),
                    ("s", "save"),
                    ("e", "edit"),
                    ("n", "rename"),
                    ("x", "clear"),
                ],
                vec![("Esc", "back"), help],
            ),
            PresetMode::List => (
                vec![
                    ("⏎", "recall"),
                    ("e", "edit"),
                    ("n", "rename"),
                    ("s", "save"),
                    ("x", "clear"),
                    ("Space", "output"),
                ],
                vec![("Esc", "back"), help],
            ),
        },
        _ => match &model.protection_state().mode {
            ProtectionMode::Editing(edit) if edit.typed.is_some() => {
                (vec![("⏎", "apply")], vec![("Esc", "cancel")])
            }
            ProtectionMode::Editing(_) if compact => (
                vec![("⏎", "apply"), ("↑↓", "value"), ("←→", "digit")],
                vec![("Esc", "cancel"), help],
            ),
            ProtectionMode::Editing(_) => (
                vec![
                    ("←→", "digit"),
                    ("↑↓", "value"),
                    ("⏎", "apply"),
                    ("0-9", "type"),
                ],
                vec![("Esc", "cancel"), help],
            ),
            ProtectionMode::Selecting => (
                vec![
                    ("↑↓", "limit"),
                    ("⏎", "edit"),
                    ("r", "default"),
                    ("Space", "output"),
                ],
                vec![("Esc", "back"), help],
            ),
        },
    }
}

// ---- Presets --------------------------------------------------------------------------

fn slot_title(model: &Model, slot: usize) -> String {
    let name = &model.names()[slot];
    if name.is_empty() {
        format!("M{}", slot + 1)
    } else {
        format!("M{} · {name}", slot + 1)
    }
}

/// Voltage and current shown for the slot in the edit box: the candidate
/// while editing, else what the slot holds.
fn slot_values(model: &Model) -> (u32, u32) {
    match &model.presets_state().mode {
        PresetMode::Edit(edit) => (edit.voltage, edit.current),
        _ => model
            .preset(model.presets_state().selected)
            .unwrap_or((0, 0)),
    }
}

fn watts(voltage: u32, current: u32) -> String {
    format!(
        "{:5.1}",
        f64::from(voltage) * f64::from(current) / 1_000_000.0
    )
}

fn presets_fluid(p: &mut Painter, model: &Model, view: &View, body: Rect) {
    let list_w = body.width * 6 / 10;
    let top_h = body.height.saturating_sub(3);
    let list = Rect::new(body.x, body.y, list_w, top_h);
    let edit = Rect::new(body.x + list_w, body.y, body.width - list_w, top_h);
    let check = Rect::new(body.x, body.y + top_h, body.width, 3);

    // List box.
    p.boxed(
        list,
        ("Presets", S::Ti),
        Some(("device M1–M6", S::D)),
        S::Bd,
    );
    let inner = Rect::new(list.x + 2, list.y + 1, list.width - 4, list.height - 2);
    let name_w = 11 + usize::from(inner.width).saturating_sub(44);
    p.text(
        inner.x,
        inner.y,
        &format!(
            "   {:<4}{:<name_w$}{:>8}{:>8}{:>9}",
            "#", "name", "V", "I", "P max"
        ),
        S::D,
    );
    let selected = model.presets_state().selected;
    for slot in 0..6 {
        let y = inner.y + 1 + slot as u16;
        let (voltage, current) = model.preset(slot).unwrap_or((0, 0));
        let blocked = view.live_max_voltage.is_some_and(|max| voltage > max.get());
        let name = format!("{:<name_w$}", model.names()[slot]);
        let v_text = format!("{:>6} V", two(voltage));
        let i_text = format!("  {:>4} A", two(current));
        let p_text = format!("  {} W", watts(voltage, current));
        if slot == selected {
            p.sel = true;
            p.fill(Rect::new(inner.x, y, inner.width, 1), S::Tx);
            p.spans(
                inner.x,
                y,
                &[
                    (" ▸ ", S::Set),
                    (&format!("M{}  ", slot + 1), S::Tx),
                    (&name, S::Tx),
                    (&v_text, S::Tx),
                    (&i_text, S::Tx),
                    (&p_text, S::Tx),
                ],
            );
            p.sel = false;
        } else {
            p.spans(
                inner.x,
                y,
                &[
                    ("   ", S::Page),
                    (&format!("M{}  ", slot + 1), S::Tx),
                    (&name, S::Tx),
                    (&v_text, if blocked { S::Wn } else { S::Ch1 }),
                    (&i_text, S::Ch2),
                    (&p_text, S::Pw),
                ],
            );
        }
    }
    let now_y = inner.y + 8;
    if let (Some(v), Some(i)) = (view.set_voltage, view.set_current) {
        let matches = match model.matching_preset() {
            Some(slot) => format!("  (matches M{})", slot + 1),
            None => "  (no preset matches)".to_string(),
        };
        p.spans(
            inner.x,
            now_y,
            &[
                ("Now ", S::D),
                (&format!("{} V", app::limits::two_decimals(v.get())), S::Set),
                (" / ", S::D),
                (&format!("{} A", fixed_amps(i.get())), S::Set),
                (&matches, S::D),
            ],
        );
    }
    p.text(
        inner.x,
        now_y + 1,
        "Recall sets V and I; output unchanged.",
        S::D,
    );

    preset_edit_box(p, model, edit);

    // Check box.
    let blocked = (0..6).find(|slot| model.preset_blocked(*slot).is_some());
    match blocked {
        Some(slot) => {
            let reason = model
                .preset_blocked(slot)
                .map(|b| b.standard)
                .unwrap_or_default();
            p.boxed(check, (&slot_title(model, slot), S::Ti), None, S::Bd);
            p.spans(
                check.x + 2,
                check.y + 1,
                &[
                    ("! ", S::Wn),
                    (&format!("{reason}. Recall is blocked."), S::Wn),
                ],
            );
        }
        None => {
            p.boxed(check, ("Check", S::Ti), None, S::Bd);
            p.text(
                check.x + 2,
                check.y + 1,
                "All presets are within the live limits.",
                S::D,
            );
        }
    }
}

fn two(milli: u32) -> String {
    app::limits::two_decimals(milli)
}

/// `%4.2f` of a current in A.
fn fixed_amps(milli: u32) -> String {
    format!("{}.{:03}", milli / 1000, milli % 1000)
}

/// The font for a block of `lines` lines and a text `text` in an inner area
/// `width` × `height` (§8.5).
fn pick(text: &str, extra: usize, lines: impl Fn(usize) -> usize, inner: Rect) -> Font {
    for candidate in [font::F7, font::F5, font::F3] {
        if 1 + candidate.width(text) + extra <= usize::from(inner.width)
            && lines(candidate.height()) <= usize::from(inner.height)
        {
            return candidate;
        }
    }
    font::F3
}

fn preset_edit_box(p: &mut Painter, model: &Model, rect: Rect) {
    let slot = model.presets_state().selected;
    let (voltage, current) = slot_values(model);
    let renaming = match &model.presets_state().mode {
        PresetMode::Rename(text) => Some(text.clone()),
        _ => None,
    };
    let title = match &renaming {
        Some(text) => format!("M{} · {text}", slot + 1),
        None => slot_title(model, slot),
    };
    let title_style = if renaming.is_some() { S::Sel } else { S::Ti };
    p.boxed(rect, (&title, title_style), Some(("edit", S::D)), S::Bd);

    let inner = Rect::new(rect.x + 2, rect.y + 1, rect.width - 4, rect.height - 2);
    let v_text = format::set_voltage(protocol::value::Millivolts::new(voltage));
    let i_text = format::set_current(protocol::value::Milliamps::new(current));
    let font = pick(&v_text, 0, |h| 2 * h + 5, inner);
    let h = font.height() as u16;
    let spare = inner.height.saturating_sub(2 * h + 5);
    let y0 = inner.y + spare / 2;
    let margin = inner.x + 1;

    let (editing, focus, cursors) = match &model.presets_state().mode {
        PresetMode::Edit(edit) => (true, edit.focus, edit.cursors),
        _ => (false, Focus::Voltage, [1, 1]),
    };
    p.text(inner.x, y0, "VOLTAGE", S::D);
    for (row, line) in font.render(&v_text).iter().enumerate() {
        p.text(margin, y0 + 1 + row as u16, line, S::Set);
    }
    if !editing || focus == Focus::Voltage {
        let index = [0, 1, 3, 4][cursors[0].min(3)];
        p.text(
            margin + font.start_of(&v_text, index) as u16,
            y0 + 1 + h,
            &font.caret(),
            S::Cur,
        );
    }
    p.text(inner.x, y0 + h + 2, "CURRENT", S::D);
    for (row, line) in font.render(&i_text).iter().enumerate() {
        p.text(margin, y0 + h + 3 + row as u16, line, S::Set);
    }
    if editing && focus == Focus::Current {
        let index = [0, 2, 3, 4][cursors[1].min(3)];
        p.text(
            margin + font.start_of(&i_text, index) as u16,
            y0 + 2 * h + 3,
            &font.caret(),
            S::Cur,
        );
    }
    match model.preset_validation(voltage, current) {
        Ok(text) => p.spans(inner.x, y0 + 2 * h + 4, &[("✓ ", S::Ok), (&text, S::D)]),
        Err(text) => p.spans(inner.x, y0 + 2 * h + 4, &[("! ", S::Wn), (&text, S::Wn)]),
    };
}

fn presets_compact(p: &mut Painter, model: &Model, view: &View, x0: u16, y: u16) {
    let list = Rect::new(x0, y, COMPACT_WIDTH, 8);
    p.boxed(list, ("Presets", S::Ti), None, S::Bd);
    let inner = Rect::new(list.x + 2, list.y + 1, 40, 6);
    let selected = model.presets_state().selected;
    for slot in 0..6 {
        let row_y = inner.y + slot as u16;
        let (voltage, current) = model.preset(slot).unwrap_or((0, 0));
        let text = format!(
            "M{} {:<9}{:>5}V {:>4}A",
            slot + 1,
            model.names()[slot],
            two(voltage),
            two(current)
        );
        if slot == selected {
            p.sel = true;
            p.fill(Rect::new(inner.x, row_y, inner.width, 1), S::Tx);
            p.spans(inner.x, row_y, &[(" ▸ ", S::Set), (&text, S::Tx)]);
            p.sel = false;
        } else {
            p.spans(inner.x, row_y, &[("   ", S::Page), (&text, S::Tx)]);
        }
    }

    let slot = selected;
    let (voltage, current) = slot_values(model);
    let edit = Rect::new(x0, y + 8, COMPACT_WIDTH, 4);
    let renaming = match &model.presets_state().mode {
        PresetMode::Rename(text) => Some(text.clone()),
        _ => None,
    };
    let title = match &renaming {
        Some(text) => format!("M{} · {text}", slot + 1),
        None => slot_title(model, slot),
    };
    p.boxed(
        edit,
        (&title, if renaming.is_some() { S::Sel } else { S::Ti }),
        None,
        S::Bd,
    );
    let (editing, focus, cursors) = match &model.presets_state().mode {
        PresetMode::Edit(e) => (true, e.focus, e.cursors),
        _ => (false, Focus::Voltage, [1, 1]),
    };
    let v_text = format::set_voltage(protocol::value::Millivolts::new(voltage));
    let i_text = format::set_current(protocol::value::Milliamps::new(current));
    let (v_style, i_style) = if focus == Focus::Voltage {
        (S::Sel, S::Set)
    } else {
        (S::Set, S::Sel)
    };
    let x = edit.x + 2;
    p.spans(
        x,
        edit.y + 1,
        &[
            ("V ", S::D),
            (&format!(" {v_text} "), v_style),
            (" V   I ", S::D),
            (&format!(" {i_text} "), i_style),
            (" A", S::D),
        ],
    );
    let validation = model.preset_validation(voltage, current);
    if editing {
        let (offset, index) = match focus {
            Focus::Voltage => (3, [0, 1, 3, 4][cursors[0].min(3)]),
            Focus::Current => (17, [0, 2, 3, 4][cursors[1].min(3)]),
        };
        p.text(x + offset + index as u16, edit.y + 2, "▔", S::Cur);
    } else {
        match &validation {
            Ok(text) => p.spans(x, edit.y + 2, &[("✓ ", S::Ok), (text, S::D)]),
            Err(_) => {
                let reason = model
                    .preset_blocked(slot)
                    .map(|b| b.compact)
                    .unwrap_or_default();
                p.spans(x, edit.y + 2, &[("! ", S::Wn), (&reason, S::Wn)])
            }
        };
    }

    // Status row.
    let row = y + 12;
    let mut parts: Vec<(String, S)> = Vec::new();
    if let (Some(v), Some(i)) = (view.set_voltage, view.set_current) {
        parts.push(("Now ".into(), S::D));
        parts.push((format!("{} V", two(v.get())), S::Set));
        parts.push((" / ".into(), S::D));
        parts.push((format!("{} A", fixed_amps(i.get())), S::Set));
    }
    if editing {
        match &validation {
            Ok(text) => {
                parts.push(("  ✓ ".into(), S::Ok));
                parts.push((text.clone(), S::D));
            }
            Err(_) => {
                parts.push(("  ! ".into(), S::Wn));
                parts.push((
                    model
                        .preset_blocked(slot)
                        .map(|b| b.compact)
                        .unwrap_or_default(),
                    S::Wn,
                ));
            }
        }
    } else if let Some(blocked) = (0..6).find(|s| model.preset_blocked(*s).is_some()) {
        let reason = model
            .preset_blocked(blocked)
            .map(|b| b.compact)
            .unwrap_or_default();
        parts.push((format!("  ! M{} {reason}", blocked + 1), S::Wn));
    } else if let Some(matching) = model.matching_preset() {
        parts.push((format!("  (= M{})", matching + 1), S::D));
    }
    let mut column = x0 + 1;
    for (text, style) in &parts {
        column = p.text(column, row, text, *style);
    }
}

// ---- Protection ----------------------------------------------------------------------------

fn percent(fraction: f32) -> u32 {
    if fraction.is_finite() {
        (fraction.max(0.0) * 100.0).round() as u32
    } else {
        0
    }
}

fn protection_fluid(p: &mut Painter, model: &Model, view: &View, body: Rect) {
    let Some(snapshot) = model.snapshot.as_ref() else {
        return;
    };
    let dump = &snapshot.dump;
    let ctx = model.protection_context();
    let violation = ctx.and_then(|ctx| protection::violation(dump, &ctx));
    let limits_box = Rect::new(body.x, body.y, body.width, 10);
    p.boxed(
        limits_box,
        ("Protection limits", S::Ti),
        Some(("trip turns output off", S::D)),
        S::Bd,
    );
    let inner = Rect::new(limits_box.x + 2, limits_box.y + 1, limits_box.width - 4, 8);
    p.text(
        inner.x,
        inner.y,
        &format!(
            "   {:<5}{:>8}   {:<13}{:<15}load",
            "name", "limit", "device range", "live"
        ),
        S::D,
    );
    let bar_w = 16 + usize::from(body.width).saturating_sub(80);
    let selected = model.protection_state().selected;
    for (index, limit) in ALL.iter().enumerate() {
        let y = inner.y + 1 + index as u16;
        let value = model.limit(*limit).unwrap_or(0);
        let live = limit.live_text(dump);
        let violates = violation.as_ref().is_some_and(|(l, _)| l == limit);
        let limit_style = if violates { S::Er } else { S::Set };
        let percent = percent(limit.load(dump, value));
        let range = format!("   {:<13}", limit.range_text(limit.range_max(dump)));
        let row_selected = index == selected;
        if row_selected {
            p.sel = true;
            p.fill(Rect::new(inner.x, y, inner.width, 1), S::Tx);
        }
        let marker = if row_selected {
            (" ▸ ", S::Set)
        } else {
            ("   ", S::Page)
        };
        let dim = S::D;
        let column = p.spans(
            inner.x,
            y,
            &[
                marker,
                (&format!("{}  ", limit.name()), S::Tx),
                (&format!("{:>8}", limit.text(value)), limit_style),
                (&range, if row_selected { S::Tx } else { dim }),
                (&format!("{:<5}", limit.live_label()), dim),
                (&format!("{live:>8}"), S::Tx),
                ("       ", S::Page),
            ],
        );
        let fraction = limit.load(dump, value);
        let bar_style = match limit {
            Limit::Ovp => S::Ch1,
            Limit::Ocp => S::Ch2,
            Limit::Opp => S::Pw,
            _ => S::Ti,
        };
        let end = p.bar(column, y, fraction, bar_w, bar_style);
        p.text(end, y, &format!(" {percent:>3}%   "), dim);
        p.sel = false;
    }
    let status_y = inner.y + 7;
    let lamp = |p: &mut Painter, text: &str, style: S, tail: &str| {
        p.spans(
            inner.x,
            status_y,
            &[("● ", style), (text, S::Tx), (tail, S::D)],
        );
    };
    if let Some((_, text)) = &violation {
        p.spans(inner.x, status_y, &[("✗ ", S::Er), (text, S::Er)]);
    } else if view.tripped() {
        let name = ALL
            .get(usize::from(view.protection).wrapping_sub(1))
            .map_or("", |l| l.name());
        p.spans(
            inner.x,
            status_y,
            &[
                ("● ", S::Er),
                (&format!("{name} tripped"), S::Er),
                (&format!("   last trip: {name}"), S::D),
            ],
        );
    } else {
        lamp(p, "no protection tripped", S::Ok, "   last trip: none");
    }

    // Edit box.
    let edit_box = Rect::new(
        body.x,
        body.y + 10,
        body.width,
        body.height.saturating_sub(10),
    );
    protection_edit_box(p, model, edit_box);
}

fn protection_edit_box(p: &mut Painter, model: &Model, rect: Rect) {
    let Some(snapshot) = model.snapshot.as_ref() else {
        return;
    };
    let dump = &snapshot.dump;
    let limit = ALL[model.protection_state().selected.min(4)];
    let (editing, candidate, cursor, typed) = match &model.protection_state().mode {
        ProtectionMode::Editing(edit) => (true, edit.candidate, edit.cursor, edit.typed.clone()),
        ProtectionMode::Selecting => (
            false,
            model.limit(limit).unwrap_or(0),
            limit.initial_digit(),
            None,
        ),
    };
    let (title, right) = if editing {
        (
            format!("Editing {}", limit.name()),
            "←→ digit  ↑↓ value  ⏎ apply  Esc cancel",
        )
    } else {
        (
            format!("Selected {}", limit.name()),
            "⏎ edit  r device default",
        )
    };
    p.boxed(rect, (&title, S::Ti), Some((right, S::D)), S::Bd);
    let inner = Rect::new(
        rect.x + 2,
        rect.y + 1,
        rect.width - 4,
        rect.height.saturating_sub(2),
    );
    let text = typed
        .as_ref()
        .map_or_else(|| limit.digits(candidate), |t| t.text.clone());
    let font = pick(&limit.digits(candidate), 3 + 45, |h| h + 1, inner);
    let h = font.height() as u16;
    let spare = inner.height.saturating_sub(h + 1);
    let y0 = inner.y + spare / 2;
    let margin = inner.x + 1;
    let digit_style = match &typed {
        Some(t) if t.rejected => S::Er,
        _ => S::Set,
    };
    for (row, line) in font.render(&text).iter().enumerate() {
        p.text(margin, y0 + row as u16, line, digit_style);
    }
    if editing && typed.is_none() {
        let digits = limit.digits(candidate);
        let x = margin + font.start_of(&digits, limit.char_of_digit(cursor)) as u16;
        p.text(x, y0 + h, &font.caret(), S::Cur);
    }

    // The three info lines to the right of the digits.
    let Some(ctx) = model.protection_context() else {
        return;
    };
    let x = margin + font.width(&limit.digits(candidate)) as u16 + 3;
    let check = protection::check(limit, candidate, dump, &ctx);
    let (label, value) = protection::related(limit, &ctx);
    p.spans(
        x,
        y0,
        &[
            (&format!("{:<15}", "Device range"), S::D),
            (&limit.range_text(limit.range_max(dump)), S::Tx),
        ],
    );
    let (mark, mark_style) = if check.ok {
        ("✓ ", S::Ok)
    } else {
        ("✗ ", S::Er)
    };
    p.spans(
        x,
        y0 + 1,
        &[
            (&format!("{label:<15}"), S::D),
            (&value, S::Tx),
            ("   ", S::D),
            (mark, mark_style),
            (&check.text, if check.ok { S::D } else { S::Er }),
        ],
    );
    let (drawn_label, drawn, margin_text) = protection::drawn(limit, dump, candidate, &ctx);
    let tail = if margin_text.is_empty() {
        String::new()
    } else {
        format!("   {margin_text}")
    };
    p.spans(
        x,
        y0 + 2,
        &[
            (&format!("{drawn_label:<15}"), S::D),
            (&drawn, S::Tx),
            (&tail, S::D),
        ],
    );
    let from_default = matches!(
        &model.protection_state().mode,
        ProtectionMode::Editing(edit) if edit.from_default
    );
    if from_default && model_default_is_max(limit, candidate, dump) {
        p.spans(
            x,
            y0 + 3,
            &[
                ("! ", S::Wn),
                ("default is the device maximum: no protection", S::Wn),
            ],
        );
    }
}

/// `r` loaded a default that equals the device maximum: flagged, but allowed
/// (§8.3).
fn model_default_is_max(limit: Limit, candidate: u32, dump: &protocol::Dump) -> bool {
    candidate == limit.default_value() && candidate == limit.range_max(dump)
}

fn protection_compact(p: &mut Painter, model: &Model, view: &View, x0: u16, y: u16) {
    let Some(snapshot) = model.snapshot.as_ref() else {
        return;
    };
    let dump = &snapshot.dump;
    let table = Rect::new(x0, y, COMPACT_WIDTH, 7);
    p.boxed(table, ("Protection", S::Ti), None, S::Bd);
    let inner = Rect::new(table.x + 2, table.y + 1, 40, 5);
    let selected = model.protection_state().selected;
    for (index, limit) in ALL.iter().enumerate() {
        let row_y = inner.y + index as u16;
        let value = model.limit(*limit).unwrap_or(0);
        let fraction = limit.load(dump, value);
        let row_selected = index == selected;
        if row_selected {
            p.sel = true;
            p.fill(Rect::new(inner.x, row_y, inner.width, 1), S::Tx);
        }
        let marker = if row_selected {
            (" ▸ ", S::Set)
        } else {
            ("   ", S::Page)
        };
        p.spans(
            inner.x,
            row_y,
            &[
                marker,
                (&format!("{} ", limit.name()), S::Tx),
                (&format!("{:>8}", limit.text(value)), S::Set),
            ],
        );
        let bar_style = match limit {
            Limit::Ovp => S::Ch1,
            Limit::Ocp => S::Ch2,
            Limit::Opp => S::Pw,
            _ => S::Ti,
        };
        let end = p.bar(inner.x + 40 - 15, row_y, fraction, 10, bar_style);
        p.text(end, row_y, &format!(" {:>3}%", percent(fraction)), S::D);
        p.sel = false;
    }

    let limit = ALL[selected.min(4)];
    let (editing, candidate, cursor, typed) = match &model.protection_state().mode {
        ProtectionMode::Editing(edit) => (true, edit.candidate, edit.cursor, edit.typed.clone()),
        ProtectionMode::Selecting => (
            false,
            model.limit(limit).unwrap_or(0),
            limit.initial_digit(),
            None,
        ),
    };
    let edit_box = Rect::new(x0, y + 7, COMPACT_WIDTH, 5);
    let title = if editing {
        format!("Editing {}", limit.name())
    } else {
        format!("Selected {}", limit.name())
    };
    p.boxed(edit_box, (&title, S::Ti), None, S::Bd);
    let x = edit_box.x + 2;
    let Some(ctx) = model.protection_context() else {
        return;
    };
    let digits = typed
        .as_ref()
        .map_or_else(|| limit.digits(candidate), |t| t.text.clone());
    let check = protection::check(limit, candidate, dump, &ctx);
    let (unit_letter, unit) = match limit {
        Limit::Ovp | Limit::Lvp => ("V", "V"),
        Limit::Ocp => ("I", "A"),
        Limit::Opp => ("P", "W"),
        Limit::Otp => ("T", "°C"),
    };
    let field_style = match &typed {
        Some(t) if t.rejected => S::Er,
        _ => S::Sel,
    };
    let (mark, mark_style) = if check.ok {
        ("✓ ", S::Ok)
    } else {
        ("✗ ", S::Er)
    };
    p.spans(
        x,
        edit_box.y + 1,
        &[
            (&format!("{unit_letter} "), S::D),
            (&format!(" {digits} "), field_style),
            (&format!(" {unit}   "), S::D),
            (mark, mark_style),
            (&check.compact, if check.ok { S::D } else { S::Er }),
        ],
    );
    if editing && typed.is_none() {
        let offset = 3 + limit.char_of_digit(cursor) as u16;
        p.text(x + offset, edit_box.y + 2, "▔", S::Cur);
    }
    let (_, drawn, _) = protection::drawn(limit, dump, candidate, &ctx);
    p.spans(
        x,
        edit_box.y + 3,
        &[
            (
                &format!("range {}", limit.range_text(limit.range_max(dump))),
                S::D,
            ),
            (&format!(" · drawn {drawn}"), S::D),
        ],
    );
    let row = y + 12;
    let violation = protection::violation(dump, &ctx);
    if let Some((_, text)) = violation {
        p.spans(x0 + 1, row, &[("✗ ", S::Er), (&text, S::Er)]);
    } else if view.tripped() {
        p.spans(x0 + 1, row, &[("● ", S::Er), ("tripped", S::Er)]);
    } else {
        p.spans(
            x0 + 1,
            row,
            &[("● ", S::Ok), ("no protection tripped", S::Ok)],
        );
    }
}
