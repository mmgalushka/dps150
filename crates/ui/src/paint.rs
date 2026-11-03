//! Small drawing helpers over a ratatui `Buffer`. Every glyph is one cell
//! wide (UI-06), so a character count is a width.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;

use crate::theme::{S, Theme};

pub(crate) struct Painter<'a> {
    pub buf: &'a mut Buffer,
    pub theme: Theme,
    /// Draw on the `sel` background: a selected row (§5.7).
    pub sel: bool,
}

impl Painter<'_> {
    /// Writes `text` at (`x`, `y`) and returns the column after it. Cells
    /// outside the buffer are skipped.
    pub fn text(&mut self, x: u16, y: u16, text: &str, s: S) -> u16 {
        let style = self.theme.style(s);
        let style = if self.sel {
            self.theme.on_sel(style)
        } else {
            style
        };
        let area = self.buf.area;
        let mut column = x;
        for ch in text.chars() {
            if column >= area.x + area.width {
                column = column.saturating_add(1);
                continue;
            }
            if y >= area.y && y < area.y + area.height && column >= area.x {
                let mut utf8 = [0u8; 4];
                let cell = &mut self.buf[(column, y)];
                cell.set_symbol(ch.encode_utf8(&mut utf8));
                cell.set_style(style);
            }
            column = column.saturating_add(1);
        }
        column
    }

    /// Writes consecutive pieces, each in its own style.
    pub fn spans(&mut self, x: u16, y: u16, spans: &[(&str, S)]) -> u16 {
        spans
            .iter()
            .fold(x, |column, (text, s)| self.text(column, y, text, *s))
    }

    /// Fills `rect` with blanks in style `s`.
    pub fn fill(&mut self, rect: Rect, s: S) {
        let style = self.theme.style(s);
        let style = if self.sel {
            self.theme.on_sel(style)
        } else {
            style
        };
        let rect = rect.intersection(self.buf.area);
        for y in rect.y..rect.y + rect.height {
            for x in rect.x..rect.x + rect.width {
                let cell = &mut self.buf[(x, y)];
                cell.set_symbol(" ");
                cell.set_style(style);
            }
        }
    }

    /// A rounded box (DESIGN.md §5.1). `title` is one styled piece; `right`
    /// is optional text before the corner.
    pub fn boxed(&mut self, rect: Rect, title: (&str, S), right: Option<(&str, S)>, border: S) {
        if rect.width < 4 || rect.height < 2 {
            return;
        }
        let width = usize::from(rect.width);
        let right_cells = right.map_or(0, |(text, _)| text.chars().count() + 3);
        // `fill` must stay at least 1: width - 3 - title - 1 - 1 - right.
        let room = width.saturating_sub(3 + 1 + 1 + 1 + right_cells);
        let mut shown: String = title.0.chars().take(room).collect();
        if title.0.chars().count() > room && room > 0 {
            shown.pop();
            shown.push('…');
        }
        let fill = width - 3 - shown.chars().count() - 1 - 1 - right_cells;
        let fill = fill.max(1);
        let (x, y) = (rect.x, rect.y);
        let mut column = self.text(x, y, "╭─ ", border);
        column = self.text(column, y, &shown, title.1);
        column = self.text(column, y, " ", border);
        column = self.text(column, y, &"─".repeat(fill), border);
        if let Some((text, s)) = right {
            column = self.text(column, y, " ", border);
            column = self.text(column, y, text, s);
            column = self.text(column, y, " ─", border);
        }
        self.text(column, y, "╮", border);
        for row in 1..rect.height - 1 {
            self.text(x, y + row, "│", border);
            self.text(x + rect.width - 1, y + row, "│", border);
        }
        let bottom = format!("╰{}╯", "─".repeat(width - 2));
        self.text(x, y + rect.height - 1, &bottom, border);
    }

    /// `bar(fraction, width, style)` of DESIGN.md §5.2.
    pub fn bar(&mut self, x: u16, y: u16, fraction: f32, width: usize, s: S) -> u16 {
        let fraction = if fraction.is_finite() {
            fraction.clamp(0.0, 1.0)
        } else {
            0.0
        };
        let mut filled = (fraction * width as f32).round() as usize;
        if fraction > 0.0 && filled == 0 {
            filled = 1;
        }
        let filled = filled.min(width);
        let column = self.text(x, y, &"━".repeat(filled), s);
        self.text(column, y, &"─".repeat(width - filled), S::Bd)
    }
}
