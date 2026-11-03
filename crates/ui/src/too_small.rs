//! The too-small screen (DESIGN.md §2.4).

use app::Model;
use ratatui::layout::Rect;

use crate::paint::Painter;
use crate::theme::S;

pub(crate) fn draw(p: &mut Painter, model: &Model, area: Rect) {
    let dump = model.snapshot.as_ref().map(|s| &s.dump);
    let (output, style) = match (model.is_online(), dump) {
        (false, _) => ("output state unknown".to_string(), S::Er),
        (true, Some(dump)) if dump.output_on => (
            format!(
                "output ON {} V / {} A",
                two_decimals(dump.set_voltage.get()),
                three_decimals(dump.set_current.get())
            ),
            S::Wn,
        ),
        _ => ("output OFF".to_string(), S::D),
    };
    let lines = [
        ("Terminal too small".to_string(), S::Wn),
        (
            format!("need 44×16, have {}×{}", area.width, area.height),
            S::D,
        ),
        (output, style),
    ];
    let top = area.y + area.height.saturating_sub(3) / 2;
    for (row, (text, s)) in lines.iter().enumerate() {
        let width = text.chars().count() as u16;
        let x = area.x + area.width.saturating_sub(width) / 2;
        p.text(x, top + row as u16, text, *s);
    }
}

fn two_decimals(milli: u32) -> String {
    let hundredths = (milli + 5) / 10;
    format!("{}.{:02}", hundredths / 100, hundredths % 100)
}

fn three_decimals(milli: u32) -> String {
    format!("{}.{:03}", milli / 1000, milli % 1000)
}
