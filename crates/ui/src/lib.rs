//! ratatui rendering of the screens (DESIGN.md). One function from the model
//! to cells; it touches no terminal.
#![forbid(unsafe_code)]

pub mod font;
pub mod format;
pub mod layout;
mod main_panel;
mod paint;
pub mod theme;
mod too_small;

use app::Model;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

pub use layout::Tier;
pub use theme::{Depth, S, Theme, detect_depth};

/// Draws the whole screen for `model` into `buf` (the buffer's area is the
/// terminal). Layout, band and digit font follow the size (UI-01, UI-12).
pub fn render(model: &Model, buf: &mut Buffer, theme: Theme) {
    let area = buf.area;
    let mut painter = paint::Painter { buf, theme };
    painter.fill(Rect::new(area.x, area.y, area.width, area.height), S::Page);
    match Tier::select(area.width, area.height) {
        Tier::TooSmall => too_small::draw(&mut painter, model, area),
        tier => main_panel::draw(&mut painter, model, area, tier),
    }
}
