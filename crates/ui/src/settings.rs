//! The Settings screen (DESIGN.md §5.11, §8.4, §8.5).

use app::{Model, Row, SLIDER_MAX};
use ratatui::layout::Rect;

use crate::layout::COMPACT_WIDTH;
use crate::main_panel::View;
use crate::paint::Painter;
use crate::theme::S;

fn row_start(p: &mut Painter, rect: Rect, y: u16, selected: bool) {
    if selected {
        p.sel = true;
        p.fill(Rect::new(rect.x + 1, y, rect.width - 2, 1), S::Tx);
    }
}

fn marker(p: &mut Painter, x: u16, y: u16, selected: bool) {
    if selected {
        p.text(x + 1, y, "▸", S::Set);
    }
}

fn slider(p: &mut Painter, model: &Model, rect: Rect, y: u16, row: Row) {
    let selected = model.settings_selected() == row;
    row_start(p, rect, y, selected);
    let x = rect.x + 2;
    marker(p, x, y, selected);
    p.text(x + 3, y, row.label(), S::Tx);
    let level = model.slider(row);
    let right = rect.x + rect.width - 2;
    let bar = right - 15;
    p.bar(bar, y, f32::from(level) / f32::from(SLIDER_MAX), 10, S::Set);
    p.text(bar + 10, y, &format!("  {level}  "), S::Tx);
    p.sel = false;
}

fn option(
    p: &mut Painter,
    model: &Model,
    rect: Rect,
    y: u16,
    row: Option<Row>,
    label: &str,
    value: &str,
) {
    let selected = row.is_some_and(|row| model.settings_selected() == row);
    row_start(p, rect, y, selected);
    let x = rect.x + 2;
    marker(p, x, y, selected);
    let arrow = if selected { S::Tx } else { S::D };
    p.spans(
        x + 3,
        y,
        &[
            (&format!("{label:<12}"), S::Tx),
            ("◂ ", arrow),
            (value, S::Set),
            (" ▸", arrow),
        ],
    );
    p.sel = false;
}

fn readonly(p: &mut Painter, rect: Rect, y: u16, label: &str, value: &str, value_style: S) {
    let x = rect.x + 2;
    p.text(x, y, label, S::D);
    let right = rect.x + rect.width - 2;
    p.text(right - value.chars().count() as u16, y, value, value_style);
}

fn kv(model: &Model, row: Row) -> (&'static str, String) {
    (row.label(), model.option_text(row))
}

fn opt(p: &mut Painter, model: &Model, rect: Rect, y: u16, row: Row) {
    let (label, value) = kv(model, row);
    option(p, model, rect, y, Some(row), label, &value);
}

fn limits_text(model: &Model) -> String {
    let Some(snapshot) = model.snapshot.as_ref() else {
        return "-".into();
    };
    let dump = &snapshot.dump;
    let trim = |milli: u32| {
        let text = format!("{}.{}", milli / 1000, (milli % 1000) / 100);
        text.strip_suffix(".0").map_or(text.clone(), str::to_string)
    };
    format!(
        "{} V · {} A · {} W",
        trim(dump.ovp_range.get()),
        trim(dump.ocp_range.get()),
        trim(dump.opp_range.get())
    )
}

fn basename(port: &str) -> &str {
    let name = port.rsplit('/').next().unwrap_or(port);
    name.strip_prefix("cu.").unwrap_or(name)
}

pub(crate) fn fluid(p: &mut Painter, model: &Model, view: &View, body: Rect) {
    let left_w = body.width - body.width / 2;
    let top_h = body.height.div_ceil(2);
    let bottom_h = body.height / 2;
    let device = Rect::new(body.x, body.y, left_w, top_h);
    let safety = Rect::new(body.x + left_w, body.y, body.width / 2, top_h);
    let display = Rect::new(body.x, body.y + top_h, left_w, bottom_h);
    let connection = Rect::new(body.x + left_w, body.y + top_h, body.width / 2, bottom_h);

    p.boxed(
        device,
        ("Device", S::Ti),
        Some(("saved on device", S::D)),
        S::Bd,
    );
    slider(p, model, device, device.y + 1, Row::Brightness);
    slider(p, model, device, device.y + 2, Row::Volume);
    let snapshot = model.snapshot.as_ref();
    let text = |value: Option<&String>| value.cloned().unwrap_or_else(|| "-".into());
    readonly(p, device, device.y + 4, "Model", &view.model_name, S::Tx);
    readonly(
        p,
        device,
        device.y + 5,
        "Hardware",
        &text(snapshot.and_then(|s| s.hardware.as_ref())),
        S::Tx,
    );
    readonly(
        p,
        device,
        device.y + 6,
        "Firmware",
        &text(snapshot.and_then(|s| s.firmware.as_ref())),
        S::Tx,
    );
    readonly(
        p,
        device,
        device.y + 7,
        "Limits",
        &limits_text(model),
        S::Tx,
    );

    p.boxed(safety, ("Safety", S::Ti), None, S::Bd);
    for (i, row) in [Row::OutputOn, Row::VJump, Row::IJump, Row::OnExit]
        .into_iter()
        .enumerate()
    {
        opt(p, model, safety, safety.y + 1 + i as u16, row);
    }
    p.text(
        safety.x + 2,
        safety.y + 6,
        "Out-of-range values are rejected;",
        S::D,
    );
    p.text(
        safety.x + 2,
        safety.y + 7,
        "steps stop at the limits. Always on.",
        S::D,
    );

    p.boxed(display, ("Display", S::Ti), None, S::Bd);
    option(
        p,
        model,
        display,
        display.y + 1,
        None,
        "Theme",
        "Midnight Slate",
    );
    opt(p, model, display, display.y + 2, Row::Colours);
    readonly(
        p,
        display,
        display.y + 3,
        "Set precision",
        "10 mV / 1 mA",
        S::Tx,
    );
    opt(p, model, display, display.y + 4, Row::Refresh);

    p.boxed(connection, ("Connection", S::Ti), None, S::Bd);
    readonly(
        p,
        connection,
        connection.y + 1,
        "Port",
        basename(&model.port),
        S::Tx,
    );
    readonly(p, connection, connection.y + 2, "Baud", "115200", S::Tx);
    opt(p, model, connection, connection.y + 3, Row::AutoDetect);
    opt(p, model, connection, connection.y + 4, Row::Reconnect);
    let (link, style) = if model.is_online() {
        ("● connected", S::Ok)
    } else {
        ("○ no link", S::Er)
    };
    readonly(p, connection, connection.y + 6, "Link", link, style);
}

pub(crate) fn compact(p: &mut Painter, model: &Model, x0: u16, y: u16) {
    let rect = Rect::new(x0, y, COMPACT_WIDTH, 10);
    p.boxed(rect, ("Settings", S::Ti), None, S::Bd);
    for (i, row) in model.rows().into_iter().enumerate() {
        let row_y = rect.y + 1 + i as u16;
        if matches!(row, Row::Brightness | Row::Volume) {
            slider(p, model, rect, row_y, row);
        } else {
            opt(p, model, rect, row_y, row);
        }
    }
}
