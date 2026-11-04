//! The help overlay (DESIGN.md §5.13): it replaces the body, keeps the header
//! and, on the secondary screens, the live strip.

use app::{Model, Screen};
use ratatui::layout::Rect;

use crate::layout::{COMPACT_WIDTH, Tier};
use crate::main_panel::{View, compact_header, draw_key_items, header};
use crate::paint::Painter;
use crate::secondary;
use crate::theme::S;

/// Width of the left column of the Standard and Large overlay.
const LEFT: u16 = 38;

pub(crate) fn draw(p: &mut Painter, model: &Model, area: Rect, tier: Tier) {
    let compact = tier == Tier::Compact;
    let view = View::new(model, compact);
    let help = app::help_for(model.screen());
    let secondary = model.screen() != Screen::Main;
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
    if secondary {
        let name = secondary::screen_name(model.screen());
        secondary::strip(p, &view, area, x0, width, name);
    }
    let top = area.y + if secondary { 2 } else { 1 };
    let (bar_y, height) = if compact {
        // The Compact frame is 14 rows on the main panel and 16 elsewhere.
        let bar = area.y + if secondary { 15 } else { 13 };
        (bar, bar - top)
    } else {
        let bar = area.y + area.height - 1;
        (bar, bar - top)
    };
    let rect = Rect::new(x0, top, width, height);
    if compact {
        compact_box(p, &help, rect, model.help_page().unwrap_or(0));
    } else {
        fluid_box(p, &help, rect);
    }
    draw_key_items(p, x0, bar_y, &[("Esc", "close")]);
}

fn fluid_box(p: &mut Painter, help: &app::Help, rect: Rect) {
    let title = format!("Help · {}", help.name);
    p.boxed(
        rect,
        (&title, S::Ti),
        Some(("? or Esc closes", S::D)),
        S::Bd,
    );
    let inner = Rect::new(
        rect.x + 2,
        rect.y + 1,
        rect.width.saturating_sub(4),
        rect.height.saturating_sub(2),
    );
    p.text(inner.x, inner.y, "Edit", S::Ti);
    p.text(inner.x + LEFT, inner.y, "Screens", S::Ti);
    let rows = help.edit.len().max(help.screens.len());
    for (index, (key, text)) in help.edit.iter().enumerate() {
        p.spans(
            inner.x,
            inner.y + 1 + index as u16,
            &[(&format!("{key:<9}"), S::Set), (text, S::Tx)],
        );
    }
    for (index, line) in help.screens.iter().enumerate() {
        p.text(inner.x + LEFT, inner.y + 1 + index as u16, line, S::Tx);
    }
    let glossary_y = inner.y + 2 + rows as u16;
    p.text(inner.x, glossary_y, "Glossary", S::Ti);
    for (index, line) in help.glossary.iter().enumerate() {
        p.text(inner.x, glossary_y + 1 + index as u16, line, S::Tx);
    }
}

/// One column, cut into pages by the height of the box (§5.13).
fn compact_box(p: &mut Painter, help: &app::Help, rect: Rect, page: usize) {
    // The model counts pages by the same fixed height (app::compact_pages).
    let inner_height = usize::from(rect.height.saturating_sub(2)).max(1);
    debug_assert!(
        inner_height == app::COMPACT_ROWS_MAIN || inner_height == app::COMPACT_ROWS_SECONDARY
    );
    let pages = help.compact.len().div_ceil(inner_height).max(1);
    let page = page.min(pages - 1);
    let title = if pages > 1 {
        format!("Help · {} {}/{}", help.name, page + 1, pages)
    } else {
        format!("Help · {}", help.name)
    };
    p.boxed(rect, (&title, S::Ti), None, S::Bd);
    let x = rect.x + 2;
    for (row, line) in help
        .compact
        .iter()
        .skip(page * inner_height)
        .take(inner_height)
        .enumerate()
    {
        let style = if matches!(*line, "Edit" | "Screens" | "Glossary") {
            S::Ti
        } else {
            S::Tx
        };
        p.text(x, rect.y + 1 + row as u16, line, style);
    }
}
