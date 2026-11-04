//! `config.toml` (CFG-01, APP-05). Missing file or key: defaults. Invalid or
//! unreadable file: defaults for the session plus a notice; the file is never
//! overwritten.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub enum ColorSetting {
    #[default]
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "24bit")]
    TrueColor,
    #[serde(rename = "256")]
    Indexed256,
    #[serde(rename = "16")]
    Ansi16,
    #[serde(rename = "none")]
    None,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OutputOn {
    #[default]
    Ask,
    Immediate,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OnExit {
    #[default]
    Leave,
    Off,
}

/// The keys of APP.md §4.1.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub theme: String,
    pub colors: ColorSetting,
    pub output_on: OutputOn,
    pub v_jump: f64,
    pub i_jump: f64,
    pub on_exit: OnExit,
    pub refresh_hz: u32,
    pub auto_detect: bool,
    pub reconnect: bool,
    pub port: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme: "midnight-slate".into(),
            colors: ColorSetting::Auto,
            output_on: OutputOn::Ask,
            v_jump: 5.0,
            i_jump: 2.0,
            on_exit: OnExit::Leave,
            refresh_hz: 10,
            auto_detect: true,
            reconnect: true,
            port: String::new(),
        }
    }
}

impl Config {
    /// Settings → Safety as the app uses it (SAFETY.md §2). A jump threshold
    /// of zero or less turns that prompt off.
    pub fn safety(&self) -> crate::output::Safety {
        let milli = |value: f64| {
            if value.is_finite() && value > 0.0 {
                (value * 1000.0).round().min(f64::from(u32::MAX)) as u32
            } else {
                0
            }
        };
        crate::output::Safety {
            ask_on: self.output_on == OutputOn::Ask,
            on_exit_off: self.on_exit == OnExit::Off,
            v_jump: milli(self.v_jump),
            i_jump: milli(self.i_jump),
        }
    }
}

/// The config and, when the file could not be used, the notice for the first
/// screen.
#[derive(Clone, Debug, PartialEq)]
pub struct Loaded {
    pub config: Config,
    pub notice: Option<String>,
}

/// `$XDG_CONFIG_HOME/dps150/config.toml`, else `~/.config/dps150/config.toml`
/// (also on macOS).
pub fn config_path(xdg: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    let base = match xdg.filter(|dir| !dir.is_empty()) {
        Some(dir) => PathBuf::from(dir),
        None => PathBuf::from(home.filter(|dir| !dir.is_empty())?).join(".config"),
    };
    Some(base.join("dps150").join("config.toml"))
}

/// Reads and checks the file at `path`.
pub fn load_config(path: Option<&std::path::Path>) -> Loaded {
    let defaults = |notice: Option<String>| Loaded {
        config: Config::default(),
        notice,
    };
    let Some(path) = path else {
        return defaults(None);
    };
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return defaults(None),
        Err(error) => {
            return defaults(Some(format!(
                "config.toml: cannot read ({}); using defaults",
                error.kind()
            )));
        }
    };
    parse_config(&text)
}

pub(crate) fn parse_config(text: &str) -> Loaded {
    let invalid = |line: usize| Loaded {
        config: Config::default(),
        notice: Some(format!(
            "config.toml: line {line}: invalid value; using defaults"
        )),
    };
    match toml::from_str::<Config>(text) {
        Ok(config) if matches!(config.refresh_hz, 10 | 5 | 2) => Loaded {
            config,
            notice: None,
        },
        Ok(_) => invalid(line_of(text, "refresh_hz")),
        Err(error) => {
            let line = error.span().map_or(1, |span| {
                text[..span.start.min(text.len())].lines().count().max(1)
            });
            invalid(line)
        }
    }
}

/// Writes the config (APP-05). The caller never calls this for an invalid
/// file. Comments and unknown keys in the old file are not kept.
pub fn save_config(path: &std::path::Path, config: &Config) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text = toml::to_string(config).map_err(std::io::Error::other)?;
    std::fs::write(path, text)
}

fn line_of(text: &str, key: &str) -> usize {
    text.lines()
        .position(|line| line.trim_start().starts_with(key))
        .map_or(1, |index| index + 1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn cfg_01_path_prefers_xdg_then_home_dot_config() {
        assert_eq!(
            config_path(Some("/x"), Some("/h")),
            Some(PathBuf::from("/x/dps150/config.toml"))
        );
        assert_eq!(
            config_path(None, Some("/Users/me")),
            Some(PathBuf::from("/Users/me/.config/dps150/config.toml"))
        );
        assert_eq!(
            config_path(Some(""), Some("/h")),
            Some(PathBuf::from("/h/.config/dps150/config.toml"))
        );
        assert_eq!(config_path(None, None), None);
    }

    #[test]
    fn cfg_01_a_saved_config_loads_back_the_same() {
        let dir = std::env::temp_dir().join(format!("dps150-cfg-{}", std::process::id()));
        let path = dir.join("dps150").join("config.toml");
        let config = Config {
            colors: ColorSetting::Indexed256,
            v_jump: 0.0,
            refresh_hz: 2,
            reconnect: false,
            ..Config::default()
        };
        save_config(&path, &config).unwrap();
        let loaded = load_config(Some(&path));
        assert_eq!(loaded.config, config);
        assert_eq!(loaded.notice, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cfg_01_a_missing_file_or_key_means_defaults() {
        let loaded = load_config(Some(Path::new("/nonexistent/dps150/config.toml")));
        assert_eq!(loaded.config, Config::default());
        assert_eq!(loaded.notice, None);
        let loaded = parse_config("reconnect = false\n");
        assert!(!loaded.config.reconnect);
        assert_eq!(loaded.config.refresh_hz, 10);
        assert_eq!(loaded.notice, None);
    }

    #[test]
    fn saf_03_the_safety_settings_become_thresholds_in_thousandths() {
        let safety = Config::default().safety();
        assert!(safety.ask_on);
        assert!(!safety.on_exit_off);
        assert_eq!((safety.v_jump, safety.i_jump), (5_000, 2_000));
        let loaded = parse_config("output_on = \"immediate\"\nv_jump = 0\ni_jump = 0.25\n");
        let safety = loaded.config.safety();
        assert!(!safety.ask_on);
        assert_eq!((safety.v_jump, safety.i_jump), (0, 250));
        let loaded = parse_config("v_jump = -3.0\n");
        assert_eq!(loaded.config.safety().v_jump, 0);
    }

    #[test]
    fn cfg_01_the_documented_file_parses() {
        let loaded = parse_config(
            r#"
theme    = "midnight-slate"
colors   = "24bit"
output_on = "immediate"
v_jump   = 0
i_jump   = 1.5
on_exit  = "off"
refresh_hz = 5
auto_detect = false
reconnect   = false
port = "/dev/ttyACM1"
unknown_key = 1
"#,
        );
        assert_eq!(loaded.notice, None);
        let config = loaded.config;
        assert_eq!(config.colors, ColorSetting::TrueColor);
        assert_eq!(config.output_on, OutputOn::Immediate);
        assert_eq!(config.on_exit, OnExit::Off);
        assert_eq!((config.v_jump, config.i_jump), (0.0, 1.5));
        assert_eq!(config.refresh_hz, 5);
        assert_eq!(config.port, "/dev/ttyACM1");
        assert!(!config.auto_detect);
    }

    #[test]
    fn app_05_an_invalid_file_gives_defaults_and_the_notice_with_a_line() {
        let loaded = parse_config("reconnect = true\nrefresh_hz = \"fast\"\n");
        assert_eq!(loaded.config, Config::default());
        assert_eq!(
            loaded.notice.as_deref(),
            Some("config.toml: line 2: invalid value; using defaults")
        );
        let loaded = parse_config("colors = \"rainbow\"\n");
        assert!(loaded.notice.unwrap().contains("line 1"));
        let loaded = parse_config("refresh_hz = 7\n");
        assert_eq!(loaded.config.refresh_hz, 10);
        assert!(loaded.notice.is_some(), "only 10, 5 and 2 are allowed");
    }

    #[test]
    fn app_05_an_unreadable_file_gives_defaults_and_a_notice() {
        // A directory where the file should be.
        let loaded = load_config(Some(Path::new("/")));
        assert_eq!(loaded.config, Config::default());
        assert!(
            loaded
                .notice
                .unwrap()
                .starts_with("config.toml: cannot read")
        );
    }
}
