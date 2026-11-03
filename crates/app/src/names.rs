//! `presets.toml`: the names of the six preset slots (PRE-03, CFG-02, APP.md
//! §4.2). Voltages and currents are never stored here; they live on the
//! device.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Longest slot name: it must fit the Compact name column (PRE-03).
pub const MAX_NAME: usize = 9;

/// The slot names; an empty string is "no name".
pub type Names = [String; 6];

#[derive(Serialize, Deserialize, Default)]
struct File {
    #[serde(default)]
    names: Vec<String>,
}

/// `presets.toml` next to `config.toml`.
pub fn names_path(xdg: Option<&str>, home: Option<&str>) -> Option<PathBuf> {
    let config = crate::config_path(xdg, home)?;
    Some(config.with_file_name("presets.toml"))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadedNames {
    pub names: Names,
    /// Why the file was not used, for the first screen (APP-05).
    pub notice: Option<String>,
    /// False when the file exists but is invalid: it is never overwritten.
    pub writable: bool,
}

fn blank() -> Names {
    Default::default()
}

pub fn load_names(path: Option<&Path>) -> LoadedNames {
    let unusable = |notice: String| LoadedNames {
        names: blank(),
        notice: Some(notice),
        writable: false,
    };
    let Some(path) = path else {
        return LoadedNames {
            names: blank(),
            notice: None,
            writable: true,
        };
    };
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return LoadedNames {
                names: blank(),
                notice: None,
                writable: true,
            };
        }
        Err(error) => {
            return unusable(format!(
                "presets.toml: cannot read ({}); using defaults",
                error.kind()
            ));
        }
    };
    parse_names(&text)
}

pub(crate) fn parse_names(text: &str) -> LoadedNames {
    match toml::from_str::<File>(text) {
        Ok(file) => {
            let mut names = blank();
            for (slot, name) in file.names.into_iter().take(6).enumerate() {
                names[slot] = name.chars().take(MAX_NAME).collect();
            }
            LoadedNames {
                names,
                notice: None,
                writable: true,
            }
        }
        Err(error) => {
            let line = error.span().map_or(1, |span| {
                text[..span.start.min(text.len())].lines().count().max(1)
            });
            LoadedNames {
                names: blank(),
                notice: Some(format!(
                    "presets.toml: line {line}: invalid value; using defaults"
                )),
                writable: false,
            }
        }
    }
}

/// Writes the names, creating the directory on first change.
pub fn save_names(path: &Path, names: &Names) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let file = File {
        names: names.to_vec(),
    };
    let text = toml::to_string(&file).map_err(std::io::Error::other)?;
    std::fs::write(path, text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cfg_02_the_file_sits_next_to_config_toml() {
        assert_eq!(
            names_path(Some("/x"), None),
            Some(PathBuf::from("/x/dps150/presets.toml"))
        );
        assert_eq!(
            names_path(None, Some("/Users/me")),
            Some(PathBuf::from("/Users/me/.config/dps150/presets.toml"))
        );
    }

    #[test]
    fn pre_03_names_load_up_to_nine_characters_and_six_slots() {
        let loaded = parse_names(
            r#"names = ["3V3 logic", "USB 5V", "9V pedal", "12V fan", "LED test", "24V rail", "extra"]"#,
        );
        assert_eq!(loaded.notice, None);
        assert_eq!(loaded.names[0], "3V3 logic");
        assert_eq!(loaded.names[5], "24V rail");
        let loaded = parse_names(r#"names = ["a very long name indeed"]"#);
        assert_eq!(loaded.names[0], "a very lo");
        assert_eq!(loaded.names[1], "");
    }

    #[test]
    fn app_05_a_missing_file_means_no_names_and_a_broken_one_is_never_written() {
        let loaded = load_names(Some(Path::new("/nonexistent/dps150/presets.toml")));
        assert_eq!(loaded.notice, None);
        assert!(loaded.writable);
        let loaded = parse_names("names = 3\n");
        assert!(!loaded.writable);
        assert_eq!(
            loaded.notice.as_deref(),
            Some("presets.toml: line 1: invalid value; using defaults")
        );
        assert!(loaded.names.iter().all(String::is_empty));
    }

    #[test]
    fn cfg_02_saving_and_loading_round_trip_and_only_names_are_stored() {
        let dir = std::env::temp_dir().join(format!("dps150-names-{}", std::process::id()));
        let path = dir.join("dps150").join("presets.toml");
        let mut names = blank();
        names[2] = "9V pedal".into();
        names[4] = "say \"hi\"".into();
        save_names(&path, &names).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            !text.contains('V') || text.contains("9V pedal"),
            "no setpoints"
        );
        assert!(!text.to_lowercase().contains("voltage"));
        assert_eq!(load_names(Some(&path)).names, names);
        let _ = std::fs::remove_dir_all(dir);
    }
}
