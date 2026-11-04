//! The Settings screen: rows, keys and values (DESIGN.md §8.4). Slider rows
//! write the device (`D6`, `D7`); option rows change the config, which the
//! caller saves with [`Model::take_config_to_save`] (APP-05).

use protocol::Write;

use crate::config::{ColorSetting, Config, OnExit, OutputOn};
use crate::model::{Effect, Key, Model};

/// The slider steps shown: the placeholder of DESIGN.md §8.4 until the
/// panel's own range is known.
pub const SLIDER_MAX: u8 = 5;

/// An editable row (§8.4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Row {
    Brightness,
    Volume,
    OutputOn,
    VJump,
    IJump,
    OnExit,
    Colours,
    Refresh,
    AutoDetect,
    Reconnect,
}

use Row::*;

/// Every editable row in screen order, Standard grouping.
const ALL: [Row; 10] = [
    Brightness, Volume, OutputOn, VJump, IJump, OnExit, Colours, Refresh, AutoDetect, Reconnect,
];

/// The rows of each Standard group: Device, Safety, Display, Connection.
pub const GROUPS: [&[Row]; 4] = [
    &[Brightness, Volume],
    &[OutputOn, VJump, IJump, OnExit],
    &[Colours, Refresh],
    &[AutoDetect, Reconnect],
];

/// Compact lists a subset (§8.4).
const COMPACT: [Row; 8] = [
    Brightness, Volume, OutputOn, VJump, IJump, OnExit, Refresh, Reconnect,
];

const V_JUMPS: [f64; 5] = [1.0, 2.0, 5.0, 10.0, 0.0];
const I_JUMPS: [f64; 5] = [0.5, 1.0, 2.0, 3.0, 0.0];
const REFRESH: [u32; 3] = [2, 5, 10];
const COLOURS: [ColorSetting; 5] = [
    ColorSetting::Auto,
    ColorSetting::TrueColor,
    ColorSetting::Indexed256,
    ColorSetting::Ansi16,
    ColorSetting::None,
];

impl Row {
    pub fn label(self) -> &'static str {
        match self {
            Brightness => "Brightness",
            Volume => "Volume",
            OutputOn => "Output on",
            VJump => "V jump over",
            IJump => "I jump over",
            OnExit => "On exit",
            Colours => "Colours",
            Refresh => "Refresh",
            AutoDetect => "Auto-detect",
            Reconnect => "Reconnect",
        }
    }

    fn group(self) -> usize {
        GROUPS
            .iter()
            .position(|rows| rows.contains(&self))
            .unwrap_or(0)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SettingsState {
    pub selected: Row,
    pub config: Config,
    /// False when `config.toml` is invalid: it is never overwritten.
    pub writable: bool,
    pub dirty: bool,
    /// Compact shows fewer rows, so selection skips the hidden ones.
    pub compact: bool,
    /// What `auto` found, e.g. `24-bit`.
    pub detected: String,
}

impl Default for SettingsState {
    fn default() -> Self {
        Self {
            selected: Brightness,
            config: Config::default(),
            writable: true,
            dirty: false,
            compact: false,
            detected: "24-bit".into(),
        }
    }
}

fn jump_text(value: f64, unit: &str) -> String {
    if value.is_finite() && value > 0.0 {
        format!("{value:.1} {unit}")
    } else {
        "off".into()
    }
}

/// The next choice above or below `current`; `off` (0) sorts last.
fn step_jump(current: f64, choices: &[f64], forward: bool) -> f64 {
    let key = |value: f64| {
        if value.is_finite() && value > 0.0 {
            value
        } else {
            f64::INFINITY
        }
    };
    let at = key(current);
    let mut ordered = choices.to_vec();
    ordered.sort_by(|a, b| key(*a).total_cmp(&key(*b)));
    if forward {
        ordered.into_iter().find(|c| key(*c) > at + 1e-9)
    } else {
        ordered.into_iter().rev().find(|c| key(*c) < at - 1e-9)
    }
    .unwrap_or(current)
}

impl Model {
    /// The config read at start; `writable` is false for an invalid file.
    pub fn set_config(&mut self, config: Config, writable: bool) {
        self.set_safety(config.safety());
        self.set_reconnect(config.reconnect);
        self.settings.config = config;
        self.settings.writable = writable;
    }

    pub fn config(&self) -> &Config {
        &self.settings.config
    }

    /// What `Colours = auto` detected, for the row text.
    pub fn set_detected_colours(&mut self, text: impl Into<String>) {
        self.settings.detected = text.into();
    }

    /// Compact shows fewer rows (§8.4); the caller sets it from the size.
    pub fn set_compact(&mut self, compact: bool) {
        self.settings.compact = compact;
        if !self.rows().contains(&self.settings.selected) {
            self.settings.selected = Brightness;
        }
    }

    pub fn settings_selected(&self) -> Row {
        self.settings.selected
    }

    /// The rows that can be selected now.
    pub fn rows(&self) -> Vec<Row> {
        if self.settings.compact {
            COMPACT.to_vec()
        } else {
            ALL.to_vec()
        }
    }

    /// The config to write after a change, once.
    pub fn take_config_to_save(&mut self) -> Option<Config> {
        if std::mem::take(&mut self.settings.dirty) {
            Some(self.settings.config.clone())
        } else {
            None
        }
    }

    /// The slider level shown: the pending write, else the device's.
    pub fn slider(&self, row: Row) -> u8 {
        let pending = self.pending.iter().find_map(|write| match (row, write) {
            (Brightness, Write::Brightness(level)) | (Volume, Write::Volume(level)) => Some(*level),
            _ => None,
        });
        let level = pending.or_else(|| {
            self.snapshot.as_ref().map(|s| match row {
                Brightness => s.dump.brightness,
                _ => s.dump.volume,
            })
        });
        level.unwrap_or(0).min(SLIDER_MAX)
    }

    /// The value text of an option row.
    pub fn option_text(&self, row: Row) -> String {
        let config = &self.settings.config;
        match row {
            OutputOn => match config.output_on {
                OutputOn::Ask => "ask first",
                OutputOn::Immediate => "immediately",
            }
            .into(),
            VJump => jump_text(config.v_jump, "V"),
            IJump => jump_text(config.i_jump, "A"),
            OnExit => match config.on_exit {
                OnExit::Leave => "leave output",
                OnExit::Off => "turn off",
            }
            .into(),
            Colours => match config.colors {
                ColorSetting::Auto => format!("auto · {}", self.settings.detected),
                ColorSetting::TrueColor => "24-bit".into(),
                ColorSetting::Indexed256 => "256".into(),
                ColorSetting::Ansi16 => "16".into(),
                ColorSetting::None => "off".into(),
            },
            Refresh => format!("{} Hz", config.refresh_hz),
            AutoDetect => on_off(config.auto_detect),
            Reconnect => on_off(config.reconnect),
            Brightness | Volume => self.slider(row).to_string(),
        }
    }

    pub(crate) fn settings_key(&mut self, key: Key) -> Effect {
        match key {
            Key::Esc | Key::Char('q') => self.screen = crate::screens::Screen::Main,
            Key::CtrlC => return Effect::Quit,
            Key::Up => self.move_row(-1),
            Key::Down => self.move_row(1),
            Key::Tab => self.next_group(),
            Key::Left => return self.change_setting(false),
            Key::Right => return self.change_setting(true),
            _ => {}
        }
        Effect::None
    }

    fn move_row(&mut self, by: isize) {
        let rows = self.rows();
        let at = rows
            .iter()
            .position(|row| *row == self.settings.selected)
            .unwrap_or(0);
        let to = at.saturating_add_signed(by).min(rows.len() - 1);
        self.settings.selected = rows[to];
    }

    fn next_group(&mut self) {
        let rows = self.rows();
        let group = self.settings.selected.group();
        for step in 1..=GROUPS.len() {
            let next = (group + step) % GROUPS.len();
            if let Some(row) = rows.iter().find(|row| row.group() == next) {
                self.settings.selected = *row;
                return;
            }
        }
    }

    fn change_setting(&mut self, forward: bool) -> Effect {
        let row = self.settings.selected;
        if matches!(row, Brightness | Volume) {
            return self.change_slider(row, forward);
        }
        let config = &mut self.settings.config;
        match row {
            OutputOn => {
                config.output_on = match config.output_on {
                    OutputOn::Ask => OutputOn::Immediate,
                    OutputOn::Immediate => OutputOn::Ask,
                }
            }
            VJump => config.v_jump = step_jump(config.v_jump, &V_JUMPS, forward),
            IJump => config.i_jump = step_jump(config.i_jump, &I_JUMPS, forward),
            OnExit => {
                config.on_exit = match config.on_exit {
                    OnExit::Leave => OnExit::Off,
                    OnExit::Off => OnExit::Leave,
                }
            }
            Colours => {
                let at = COLOURS
                    .iter()
                    .position(|c| *c == config.colors)
                    .unwrap_or(0);
                let len = COLOURS.len();
                config.colors = COLOURS[if forward {
                    (at + 1) % len
                } else {
                    (at + len - 1) % len
                }];
            }
            Refresh => {
                let at = REFRESH
                    .iter()
                    .position(|hz| *hz == config.refresh_hz)
                    .unwrap_or(2);
                let to = if forward {
                    (at + 1).min(2)
                } else {
                    at.saturating_sub(1)
                };
                config.refresh_hz = REFRESH[to];
            }
            AutoDetect => config.auto_detect = !config.auto_detect,
            Reconnect => config.reconnect = !config.reconnect,
            Brightness | Volume => {}
        }
        let config = self.settings.config.clone();
        self.set_safety(config.safety());
        self.set_reconnect(config.reconnect);
        if self.settings.writable {
            self.settings.dirty = true;
        } else {
            self.notify("Settings not saved: config.toml is invalid");
        }
        Effect::None
    }

    fn change_slider(&mut self, row: Row, forward: bool) -> Effect {
        // Device settings are refused while the link is down (LNK-03).
        if !self.editable() {
            return Effect::None;
        }
        let level = self.slider(row);
        let to = if forward {
            (level + 1).min(SLIDER_MAX)
        } else {
            level.saturating_sub(1)
        };
        if to == level {
            return Effect::None;
        }
        let write = if row == Brightness {
            Write::Brightness(to)
        } else {
            Write::Volume(to)
        };
        self.stage_write(write);
        Effect::Write(write)
    }
}

fn on_off(on: bool) -> String {
    if on { "on" } else { "off" }.into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screens::Screen;
    use device::Message;

    fn model() -> Model {
        let mut model = crate::model::tests::model();
        model.on_key(Key::Char(','));
        model
    }

    #[test]
    fn ui_04_comma_opens_settings_and_esc_returns() {
        let mut model = model();
        assert_eq!(model.screen(), Screen::Settings);
        model.on_key(Key::Esc);
        assert_eq!(model.screen(), Screen::Main);
    }

    #[test]
    fn ui_04_a_slider_writes_the_device_and_stops_at_its_ends() {
        let mut model = model();
        assert_eq!(
            model.on_key(Key::Right),
            Effect::Write(Write::Brightness(4))
        );
        assert_eq!(model.slider(Brightness), 4);
        for _ in 0..10 {
            model.on_key(Key::Right);
        }
        assert_eq!(model.slider(Brightness), SLIDER_MAX);
        assert_eq!(model.on_key(Key::Right), Effect::None);
    }

    #[test]
    fn ui_04_options_change_the_config_and_ask_to_save_it_once() {
        let mut model = model();
        model.on_key(Key::Tab);
        assert_eq!(model.settings_selected(), OutputOn);
        model.on_key(Key::Right);
        assert_eq!(model.option_text(OutputOn), "immediately");
        assert!(!model.safety().ask_on);
        assert!(model.take_config_to_save().is_some());
        assert!(model.take_config_to_save().is_none());
    }

    #[test]
    fn ui_04_jump_thresholds_step_through_the_list_and_end_at_off() {
        let mut model = model();
        for _ in 0..3 {
            model.on_key(Key::Down);
        }
        assert_eq!(model.settings_selected(), VJump);
        model.on_key(Key::Right);
        assert_eq!(model.option_text(VJump), "10.0 V");
        model.on_key(Key::Right);
        assert_eq!(model.option_text(VJump), "off");
        model.on_key(Key::Right);
        assert_eq!(model.option_text(VJump), "off");
        model.on_key(Key::Left);
        assert_eq!(model.option_text(VJump), "10.0 V");
    }

    #[test]
    fn ui_04_compact_skips_the_rows_it_does_not_show() {
        let mut model = model();
        model.set_compact(true);
        assert!(!model.rows().contains(&Colours));
        for _ in 0..20 {
            model.on_key(Key::Down);
        }
        assert_eq!(model.settings_selected(), Reconnect);
    }

    #[test]
    fn app_05_an_invalid_config_is_not_saved_and_says_so() {
        let mut model = model();
        model.set_config(Config::default(), false);
        model.on_key(Key::Tab);
        model.on_key(Key::Right);
        assert!(model.take_config_to_save().is_none());
        assert_eq!(
            model.message(false).unwrap().text,
            "Settings not saved: config.toml is invalid"
        );
    }

    #[test]
    fn lnk_03_device_settings_are_refused_while_the_link_is_down() {
        let mut model = model();
        model.apply(Message::Event(device::Event::Lost(
            device::Cause::MissedPolls,
        )));
        assert_eq!(model.on_key(Key::Right), Effect::None);
    }
}
