//! "Midnight Slate" (DESIGN.md §3): palette, styles and colour depth.

use app::ColorSetting;
use ratatui::style::{Color, Modifier, Style};

/// How many colours the terminal gets (UI-04).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Depth {
    TrueColor,
    Indexed256,
    Ansi16,
    None,
}

/// Picks the depth once, in the order of DESIGN.md §3.4: `NO_COLOR`, then the
/// config, then `COLORTERM`, then `TERM`.
pub fn detect_depth(
    no_color: Option<&str>,
    setting: ColorSetting,
    colorterm: Option<&str>,
    term: Option<&str>,
) -> Depth {
    if no_color.is_some_and(|value| !value.is_empty()) || setting == ColorSetting::None {
        return Depth::None;
    }
    match setting {
        ColorSetting::TrueColor => return Depth::TrueColor,
        ColorSetting::Indexed256 => return Depth::Indexed256,
        ColorSetting::Ansi16 => return Depth::Ansi16,
        ColorSetting::Auto | ColorSetting::None => {}
    }
    if matches!(colorterm, Some("truecolor" | "24bit")) {
        Depth::TrueColor
    } else if term.is_some_and(|term| term.contains("256color")) {
        Depth::Indexed256
    } else {
        Depth::Ansi16
    }
}

/// A palette token (§3.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tok {
    Bg,
    Bar,
    Tx,
    Dim,
    Bd,
    Ti,
    Ch1,
    Ch2,
    Pw,
    Set,
    Ok,
    Wn,
    Er,
    Key,
    Sel,
}

impl Tok {
    /// Hex, xterm-256 index, and the 16-colour entry (`None` = terminal
    /// default).
    fn color(self, depth: Depth) -> Option<Color> {
        let (hex, index, ansi): (u32, u8, Option<Color>) = match self {
            Self::Bg => (0x0f1420, 233, None),
            Self::Bar => (0x182030, 235, Some(Color::DarkGray)),
            Self::Tx => (0xcfd6e4, 188, None),
            Self::Dim => (0x66718a, 60, Some(Color::DarkGray)),
            Self::Bd => (0x2c364b, 237, Some(Color::DarkGray)),
            Self::Ti => (0x8ea2c8, 110, Some(Color::Gray)),
            Self::Ch1 => (0x4fd1c5, 80, Some(Color::Cyan)),
            Self::Ch2 => (0x6ea8fe, 75, Some(Color::LightBlue)),
            Self::Pw => (0xb794f6, 141, Some(Color::LightMagenta)),
            Self::Set => (0xf6c177, 216, Some(Color::LightYellow)),
            Self::Ok => (0x6fd08c, 78, Some(Color::Green)),
            Self::Wn => (0xff9e64, 215, Some(Color::Yellow)),
            Self::Er => (0xff5c7a, 204, Some(Color::LightRed)),
            Self::Key => (0x232c3f, 236, Some(Color::DarkGray)),
            Self::Sel => (0x2d3b5e, 238, Some(Color::Blue)),
        };
        match depth {
            Depth::TrueColor => Some(Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)),
            Depth::Indexed256 => Some(Color::Indexed(index)),
            Depth::Ansi16 => ansi,
            Depth::None => None,
        }
    }
}

/// A style of §3.2. Every piece of text is drawn with exactly one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum S {
    /// Default cell: `tx` on `bg`.
    Page,
    Tx,
    D,
    Bd,
    Bdf,
    Ti,
    Ch1,
    Ch2,
    Pw,
    Set,
    Cur,
    Ok,
    Wn,
    Er,
    K,
    Sel,
    Hb,
    Hd,
    Hok,
    Her,
    Hv,
    Hi,
    Hp,
    Hscr,
    Bcv,
    Bcc,
    Bon,
    Bun,
    Boff,
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    depth: Depth,
}

impl Theme {
    pub fn new(depth: Depth) -> Self {
        Self { depth }
    }

    pub fn depth(&self) -> Depth {
        self.depth
    }

    /// Foreground token, background token, bold.
    fn parts(s: S) -> (Tok, Option<Tok>, bool) {
        use Tok::*;
        match s {
            S::Page | S::Tx => (Tx, None, false),
            S::D => (Dim, None, false),
            S::Bd => (Bd, None, false),
            S::Bdf => (Ti, None, false),
            S::Ti => (Ti, None, true),
            S::Ch1 => (Ch1, None, true),
            S::Ch2 => (Ch2, None, true),
            S::Pw => (Pw, None, true),
            S::Set => (Set, None, true),
            S::Cur => (Set, None, false),
            S::Ok => (Ok, None, false),
            S::Wn => (Wn, None, true),
            S::Er => (Er, None, true),
            S::K => (Tx, Some(Key), false),
            S::Sel => (Tx, Some(Sel), true),
            S::Hb => (Tx, Some(Bar), true),
            S::Hd => (Dim, Some(Bar), false),
            S::Hok => (Ok, Some(Bar), false),
            S::Her => (Er, Some(Bar), true),
            S::Hv => (Ch1, Some(Bar), true),
            S::Hi => (Ch2, Some(Bar), true),
            S::Hp => (Pw, Some(Bar), true),
            S::Hscr => (Set, Some(Bar), true),
            S::Bcv => (Bg, Some(Ch1), true),
            S::Bcc => (Bg, Some(Wn), true),
            S::Bon => (Bg, Some(Ok), true),
            S::Bun => (Bg, Some(Wn), true),
            S::Boff => (Dim, Some(Key), false),
        }
    }

    pub fn style(&self, s: S) -> Style {
        let (fg, bg, bold) = Self::parts(s);
        if self.depth == Depth::None {
            // No colour at all: attributes only (§3.4).
            let mut style = Style::default();
            let boldish = matches!(
                s,
                S::Ti | S::Set | S::Wn | S::Er | S::Ch1 | S::Ch2 | S::Pw | S::Hb
            ) || matches!(s, S::Sel | S::Bcv | S::Bcc | S::Bon | S::Bun);
            if boldish {
                style = style.add_modifier(Modifier::BOLD);
            }
            if matches!(s, S::Sel | S::Bcv | S::Bcc | S::Bon | S::Bun) {
                style = style.add_modifier(Modifier::REVERSED);
            }
            return style;
        }
        let mut style = Style::default();
        if let Some(color) = fg.color(self.depth) {
            style = style.fg(color);
        }
        match bg {
            Some(token) => {
                if let Some(color) = token.color(self.depth) {
                    style = style.bg(color);
                }
            }
            // The page background is painted explicitly in 24-bit and 256
            // modes only; in 16 colours the terminal's own is kept (§3.4).
            None if matches!(self.depth, Depth::TrueColor | Depth::Indexed256) => {
                if let Some(color) = Tok::Bg.color(self.depth) {
                    style = style.bg(color);
                }
            }
            None => {}
        }
        if bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        style
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_04_depth_is_detected_in_the_documented_order() {
        let auto = ColorSetting::Auto;
        // NO_COLOR beats everything, even an explicit config value.
        assert_eq!(
            detect_depth(Some("1"), ColorSetting::TrueColor, Some("truecolor"), None),
            Depth::None
        );
        // An empty NO_COLOR is ignored.
        assert_eq!(
            detect_depth(Some(""), auto, Some("truecolor"), None),
            Depth::TrueColor
        );
        assert_eq!(
            detect_depth(None, ColorSetting::None, Some("truecolor"), None),
            Depth::None
        );
        // The config beats the environment.
        assert_eq!(
            detect_depth(
                None,
                ColorSetting::Ansi16,
                Some("truecolor"),
                Some("xterm-256color")
            ),
            Depth::Ansi16
        );
        assert_eq!(
            detect_depth(None, ColorSetting::Indexed256, None, None),
            Depth::Indexed256
        );
        // COLORTERM beats TERM.
        assert_eq!(
            detect_depth(None, auto, Some("24bit"), Some("xterm-256color")),
            Depth::TrueColor
        );
        assert_eq!(
            detect_depth(None, auto, None, Some("xterm-256color")),
            Depth::Indexed256
        );
        assert_eq!(detect_depth(None, auto, None, Some("xterm")), Depth::Ansi16);
        assert_eq!(detect_depth(None, auto, None, None), Depth::Ansi16);
    }

    #[test]
    fn ui_04_none_mode_emits_no_colour_and_keeps_meaning_in_attributes() {
        let theme = Theme::new(Depth::None);
        for s in [S::Page, S::Tx, S::D, S::Set, S::Er, S::Bon, S::Boff, S::Hok] {
            let style = theme.style(s);
            assert_eq!((style.fg, style.bg), (None, None), "{s:?}");
        }
        assert!(theme.style(S::Er).add_modifier.contains(Modifier::BOLD));
        assert!(
            theme
                .style(S::Bon)
                .add_modifier
                .contains(Modifier::REVERSED)
        );
        assert!(theme.style(S::Boff).add_modifier.is_empty());
    }

    #[test]
    fn ui_04_sixteen_colours_paint_no_page_background() {
        let theme = Theme::new(Depth::Ansi16);
        assert_eq!(theme.style(S::Page).bg, None);
        assert_eq!(theme.style(S::Tx).fg, None);
        assert_eq!(theme.style(S::Ch1).fg, Some(Color::Cyan));
        let theme = Theme::new(Depth::TrueColor);
        assert_eq!(theme.style(S::Page).bg, Some(Color::Rgb(0x0f, 0x14, 0x20)));
        let theme = Theme::new(Depth::Indexed256);
        assert_eq!(theme.style(S::Ch1).fg, Some(Color::Indexed(80)));
    }
}
