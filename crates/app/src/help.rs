//! The help overlay's text (DESIGN.md §5.13). Pure data: what each screen's
//! keys do, so the overlay never disagrees with the bindings it describes.

use crate::screens::Screen;

/// Help for one screen, in the two forms of §5.13.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Help {
    /// `main panel`, `presets`, `protection`, `settings`.
    pub name: &'static str,
    /// Standard and Large, left column: key and what it does.
    pub edit: Vec<(&'static str, &'static str)>,
    /// Standard and Large, right column, already laid out.
    pub screens: Vec<&'static str>,
    /// Standard and Large, below the columns.
    pub glossary: Vec<&'static str>,
    /// Compact: one column, every line; the overlay cuts it into pages.
    pub compact: Vec<&'static str>,
}

const SCREENS_STANDARD: [&str; 6] = [
    "p   Presets       ,  Settings",
    "o   Protection",
    "m 1-6   recall preset M1–M6",
    "q   quit (asks if output is ON)",
    "Space   output on / off",
    "Ctrl-X  output OFF, always",
];

/// On the other screens `q` goes back, as `Esc` does (§9.1).
const SCREENS_BACK: [&str; 6] = [
    "p   Presets       ,  Settings",
    "o   Protection",
    "m 1-6   recall preset M1–M6",
    "q   back (quits from the main panel)",
    "Space   output on / off",
    "Ctrl-X  output OFF, always",
];

const COMPACT_SCREENS: [&str; 5] = [
    "Screens",
    "p o ,   Presets Protection Settings",
    "m 1-6   recall   q   quit/back",
    "Space   output on / off",
    "Ctrl-X  output OFF, always",
];

const PROTECTIONS: [&str; 2] = [
    "OVP OCP OPP OTP LVP   over-voltage, over-current, over-power,",
    "                      over-temperature, low input voltage.",
];

fn compact(parts: &[&[&'static str]]) -> Vec<&'static str> {
    parts.iter().flat_map(|part| part.iter().copied()).collect()
}

/// Inner rows of the Compact overlay's box: 14-row main panel, 16-row
/// secondary screens (DESIGN.md §2.2, §5.13).
pub const COMPACT_ROWS_MAIN: usize = 10;
pub const COMPACT_ROWS_SECONDARY: usize = 11;

/// How many pages the Compact overlay of `screen` has.
pub fn compact_pages(screen: Screen) -> usize {
    let rows = if screen == Screen::Main {
        COMPACT_ROWS_MAIN
    } else {
        COMPACT_ROWS_SECONDARY
    };
    help_for(screen).compact.len().div_ceil(rows).max(1)
}

/// The help for `screen`.
pub fn help_for(screen: Screen) -> Help {
    match screen {
        Screen::Main => Help {
            name: "main panel",
            edit: vec![
                ("←→", "move the digit cursor"),
                ("↑↓", "change the digit"),
                ("0-9 .", "type a value, ⏎ applies"),
                ("⏎", "type a value"),
                ("Tab", "switch Voltage / Current"),
                ("v i", "type voltage / current"),
            ],
            screens: SCREENS_STANDARD.to_vec(),
            glossary: vec![
                "CV   constant voltage: the load draws less than the current limit.",
                "CC   current limiting: the current is held at the limit, voltage falls.",
                PROTECTIONS[0],
                PROTECTIONS[1],
                "A tripped protection turns the output off. Space clears it.",
            ],
            compact: compact(&[
                &[
                    "Edit",
                    "←→ digit   ↑↓ value   Tab V/I",
                    "⏎ type     v i type V / I",
                    "0-9 .   type a value",
                ],
                &COMPACT_SCREENS,
                &[
                    "",
                    "Glossary",
                    "CV constant voltage",
                    "CC current limiting",
                    "OVP OCP OPP OTP LVP: protections.",
                    "A trip turns the output off;",
                    "Space clears it.",
                ],
            ]),
        },
        Screen::Presets => Help {
            name: "presets",
            edit: vec![
                ("↑↓", "select a slot"),
                ("⏎", "recall: writes V and I"),
                ("e", "edit the slot's V and I"),
                ("n", "rename (9 characters)"),
                ("s", "save the setpoints here"),
                ("x", "clear the slot"),
                ("Esc", "back"),
            ],
            screens: SCREENS_BACK.to_vec(),
            glossary: vec![
                "Recall writes V and I and never changes the output.",
                "A slot above the live maximum is blocked, never clamped.",
            ],
            compact: compact(&[
                &[
                    "Edit",
                    "↑↓ select   ⏎ recall   e edit",
                    "n rename    s save     x clear",
                    "Esc back",
                ],
                &COMPACT_SCREENS,
                &[
                    "",
                    "Glossary",
                    "Recall writes V and I and never",
                    "changes the output. A slot above",
                    "the live maximum is blocked,",
                    "never clamped.",
                ],
            ]),
        },
        Screen::Protection => Help {
            name: "protection",
            edit: vec![
                ("↑↓", "select limit; change digit"),
                ("⏎", "edit; apply"),
                ("←→", "move the digit cursor"),
                ("0-9 .", "type a value"),
                ("r", "load the device default"),
                ("Esc", "cancel; back"),
            ],
            screens: SCREENS_BACK.to_vec(),
            glossary: vec![
                PROTECTIONS[0],
                PROTECTIONS[1],
                "A limit must stay above the setpoint it protects.",
                "The app never changes a limit by itself.",
            ],
            compact: compact(&[
                &[
                    "Edit",
                    "↑↓ select/value   ←→ digit",
                    "⏎ edit/apply      r default",
                    "0-9 .   type a value",
                    "Esc cancel/back",
                ],
                &COMPACT_SCREENS,
                &[
                    "",
                    "Glossary",
                    "OVP OCP OPP OTP LVP: protections.",
                    "A limit stays above its setpoint.",
                    "The app never changes a limit",
                    "by itself.",
                ],
            ]),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Screen; 3] = [Screen::Main, Screen::Presets, Screen::Protection];

    #[test]
    fn ui_08_each_screen_has_a_whole_number_of_compact_pages_and_more_than_one() {
        for screen in ALL {
            assert!(
                compact_pages(screen) >= 2,
                "{screen:?} needs a second page for the glossary"
            );
            assert!(compact_pages(screen) <= 3);
        }
    }

    #[test]
    fn ui_08_every_line_fits_its_layout() {
        for screen in ALL {
            let help = help_for(screen);
            for (key, text) in &help.edit {
                // Key column 9, then text, within the 38-cell left column.
                assert!(9 + text.chars().count() <= 38, "{key}: {text}");
                assert!(key.chars().count() < 9, "{key}");
            }
            for line in &help.screens {
                assert!(line.chars().count() <= 38, "{line}");
            }
            for line in &help.glossary {
                assert!(line.chars().count() <= 76, "{line}");
            }
            for line in &help.compact {
                assert!(line.chars().count() <= 40, "{line}");
            }
        }
    }

    #[test]
    fn ui_08_the_screens_column_is_the_same_everywhere_but_for_q() {
        let main = help_for(Screen::Main).screens;
        for screen in [Screen::Presets, Screen::Protection] {
            let other = help_for(screen).screens;
            assert_eq!(&main[..3], &other[..3]);
            assert_eq!(&main[4..], &other[4..]);
            assert!(other[3].contains("back"), "q goes back off the main panel");
        }
        assert!(main[3].contains("quit"));
    }

    #[test]
    fn ui_08_each_screen_documents_its_own_keys() {
        let keys = |screen| -> Vec<&'static str> {
            help_for(screen).edit.iter().map(|(k, _)| *k).collect()
        };
        assert!(keys(Screen::Presets).contains(&"n") && keys(Screen::Presets).contains(&"x"));
        assert!(keys(Screen::Protection).contains(&"r"));
        assert!(keys(Screen::Main).contains(&"Tab"));
    }

    #[test]
    fn ui_08_the_approved_presets_text_is_what_the_overlay_says() {
        let help = help_for(Screen::Presets);
        let text: Vec<String> = help.edit.iter().map(|(k, t)| format!("{k} {t}")).collect();
        assert!(text.contains(&"⏎ recall: writes V and I".to_string()));
        assert!(text.contains(&"n rename (9 characters)".to_string()));
        assert_eq!(help.compact[0], "Edit");
        assert_eq!(help.compact[1], "↑↓ select   ⏎ recall   e edit");
        assert!(
            help.compact
                .contains(&"p o ,   Presets Protection Settings")
        );
    }
}
