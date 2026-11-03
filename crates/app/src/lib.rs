//! App layer: command line, configuration, port choice and the model the
//! screens draw. No I/O of its own except reading the config file; the device
//! thread and the terminal are wired in `main` (APP.md §7).
#![forbid(unsafe_code)]

mod cli;
mod config;
pub mod constants;
pub mod limits;
mod model;
mod port;

pub use cli::{Cli, parse_args};
pub use config::{ColorSetting, Config, Loaded, OnExit, OutputOn, config_path, load_config};
pub use model::{Effect, Entry, Focus, Key, Line, LinkStatus, Model, Tone};
pub use port::{PortInfo, ResolveError, USB_ID, resolve_port};

pub mod locale {
    //! UTF-8 check (UI-10).

    /// True when the effective locale names UTF-8. The first non-empty of
    /// `LC_ALL`, `LC_CTYPE`, `LANG` decides, as in the C library.
    pub fn is_utf8(lc_all: Option<&str>, lc_ctype: Option<&str>, lang: Option<&str>) -> bool {
        [lc_all, lc_ctype, lang]
            .into_iter()
            .flatten()
            .find(|value| !value.is_empty())
            .is_some_and(|value| {
                let value = value.to_ascii_lowercase();
                value.contains("utf-8") || value.contains("utf8")
            })
    }

    #[cfg(test)]
    mod tests {
        use super::is_utf8;

        #[test]
        fn ui_10_the_first_set_variable_decides() {
            assert!(is_utf8(None, None, Some("en_US.UTF-8")));
            assert!(is_utf8(None, Some("C.utf8"), Some("C")));
            assert!(!is_utf8(
                Some("C"),
                Some("en_US.UTF-8"),
                Some("en_US.UTF-8")
            ));
            assert!(is_utf8(Some(""), None, Some("de_DE.UTF-8")));
            assert!(!is_utf8(None, None, Some("en_US.ISO-8859-1")));
            assert!(!is_utf8(None, None, None), "no locale means C");
        }
    }
}
