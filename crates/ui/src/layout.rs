//! Which layout the terminal size selects (UI-01) and which digit font fits
//! (UI-15).

use crate::font::{self, Font};

/// Width of the Compact body on the secondary screens (DESIGN.md §2.3).
pub const COMPACT_WIDTH: u16 = 44;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tier {
    /// Fluid layout, Large band: 100×21 and up.
    Large,
    /// Fluid layout, Standard band: 80×21 and up.
    Standard,
    /// Fixed 44-wide layout, 44×16 and up.
    Compact,
    TooSmall,
}

impl Tier {
    /// Tested in this order: Large, Standard, Compact, too small.
    pub fn select(cols: u16, rows: u16) -> Self {
        if cols >= 100 && rows >= 21 {
            Self::Large
        } else if cols >= 80 && rows >= 21 {
            Self::Standard
        } else if cols >= 44 && rows >= 16 {
            Self::Compact
        } else {
            Self::TooSmall
        }
    }

    pub fn is_fluid(self) -> bool {
        matches!(self, Self::Large | Self::Standard)
    }
}

/// The largest font whose content block fits both readout boxes (UI-15).
/// `inner_width` is the narrower box's inner width, `inner_height` the shared
/// inner height.
pub fn pick_font(inner_width: usize, inner_height: usize) -> Font {
    for candidate in [font::F7, font::F5, font::F3] {
        let h = candidate.height();
        let voltage = 1 + candidate.width("00.000") + 3;
        let current = 1 + candidate.width("0.000") + 3 + 13;
        if inner_height >= 2 * h + 4 && inner_width >= voltage.max(current) {
            return candidate;
        }
    }
    font::F3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_01_tiers_are_tested_in_the_documented_order() {
        assert_eq!(Tier::select(100, 21), Tier::Large);
        assert_eq!(Tier::select(250, 80), Tier::Large);
        assert_eq!(Tier::select(99, 21), Tier::Standard);
        assert_eq!(Tier::select(80, 21), Tier::Standard);
        // 80×20 is Compact, centred, rather than a clipped Fluid layout.
        assert_eq!(Tier::select(80, 20), Tier::Compact);
        assert_eq!(Tier::select(79, 40), Tier::Compact);
        assert_eq!(Tier::select(44, 16), Tier::Compact);
        assert_eq!(Tier::select(44, 15), Tier::TooSmall);
        assert_eq!(Tier::select(43, 16), Tier::TooSmall);
    }

    #[test]
    fn ui_15_the_reference_sizes_pick_the_documented_fonts() {
        // Inner width of the narrower box is cols/2 - 4; inner height is
        // rows - 9 (Large) or rows - 7 (Standard) minus the borders.
        assert_eq!(pick_font(100 / 2 - 4, 21 - 9 - 2), font::F3);
        assert_eq!(pick_font(80 / 2 - 4, 24 - 7 - 2), font::F3);
        assert_eq!(pick_font(100 / 2 - 4, 30 - 9 - 2), font::F5);
        assert_eq!(pick_font(120 / 2 - 4, 40 - 9 - 2), font::F7);
    }

    #[test]
    fn ui_15_the_font_thresholds_match_the_table() {
        // F5: inner 42 x 14. F7: inner 50 x 18.
        assert_eq!(pick_font(42, 14), font::F5);
        assert_eq!(pick_font(41, 14), font::F3);
        assert_eq!(pick_font(42, 13), font::F3);
        assert_eq!(pick_font(50, 18), font::F7);
        assert_eq!(pick_font(49, 18), font::F5);
        assert_eq!(pick_font(50, 17), font::F5);
    }
}
