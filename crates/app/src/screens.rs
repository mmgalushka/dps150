//! The Presets and Protection screens: state, keys and checks (DESIGN.md
//! §8.1, §8.3). The rendering is in `ui`.

use protocol::Write;
use protocol::value::{Milliamps, Millivolts};

use crate::limits::{self, Bounds, Parsed, Reject, Text, two_decimals};
use crate::model::{Effect, Focus, Key, Model, Tone};
use crate::names::MAX_NAME;
use crate::protection::{self, ALL, Limit};

/// A slot reset by `x` holds this (DESIGN.md §8.1).
pub const CLEARED: (u32, u32) = (5_000, 1_000);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Main,
    Presets,
    Protection,
}

/// The slot being edited with `e` (§8.1 edit mode).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresetEdit {
    pub voltage: u32,
    pub current: u32,
    pub focus: Focus,
    /// Digit cursors, as on the main panel.
    pub cursors: [usize; 2],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PresetMode {
    List,
    Edit(PresetEdit),
    /// The name field (PRE-04).
    Rename(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PresetsState {
    pub selected: usize,
    pub mode: PresetMode,
}

/// Typed text on the Protection screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Typed {
    pub text: String,
    pub rejected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtectionEdit {
    pub candidate: u32,
    pub cursor: usize,
    pub typed: Option<Typed>,
    /// `r` loaded the device default.
    pub from_default: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProtectionMode {
    Selecting,
    Editing(ProtectionEdit),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtectionState {
    pub selected: usize,
    pub mode: ProtectionMode,
}

/// A question that waits for `y` or `n` (§5.12, SAF-12).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Prompt {
    Overwrite {
        slot: usize,
        voltage: u32,
        current: u32,
    },
    Clear {
        slot: usize,
    },
    /// `Turn output ON at …?` (SAF-03).
    OutputOn {
        voltage: u32,
        current: u32,
    },
    /// `Clear OTP and turn output ON at …?` (SAF-13).
    ClearTrip {
        trip: String,
        voltage: u32,
        current: u32,
        cause: Option<String>,
    },
    /// A setpoint change beyond the threshold with the output ON (SAF-06).
    Jump {
        voltage: u32,
        current: u32,
        dv: Option<i64>,
        di: Option<i64>,
        writes: Vec<Write>,
    },
    /// A recall with the output ON (SAF-08).
    Recall {
        slot: usize,
        voltage: u32,
        current: u32,
    },
}

impl Default for PresetsState {
    fn default() -> Self {
        Self {
            selected: 0,
            mode: PresetMode::List,
        }
    }
}

impl Default for ProtectionState {
    fn default() -> Self {
        Self {
            selected: 0,
            mode: ProtectionMode::Selecting,
        }
    }
}

/// Why a slot cannot be recalled, in both text forms (§8.1, PRE-02).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Blocked {
    pub standard: String,
    pub compact: String,
}

fn amps3(milli: u32) -> String {
    format!("{}.{:03}", milli / 1000, milli % 1000)
}

impl Model {
    // ---- reading ------------------------------------------------------------

    pub fn screen(&self) -> Screen {
        self.screen
    }

    pub fn presets_state(&self) -> &PresetsState {
        &self.presets
    }

    pub fn protection_state(&self) -> &ProtectionState {
        &self.protection
    }

    pub fn names(&self) -> &crate::names::Names {
        &self.names
    }

    pub fn prompt(&self) -> Option<&Prompt> {
        self.prompt.as_ref()
    }

    /// Waiting for the digit after `m` (EDT-07).
    pub fn chord(&self) -> bool {
        self.chord
    }

    /// Slot `slot` (0..6) as shown: a pending write shows before it is read
    /// back. Voltage in mV, current in mA.
    pub fn preset(&self, slot: usize) -> Option<(u32, u32)> {
        let dump = &self.snapshot.as_ref()?.dump;
        let stored = dump.presets.get(slot)?;
        let (mut voltage, mut current) = (stored.voltage.get(), stored.current.get());
        for write in &self.pending {
            match write {
                Write::PresetVoltage(s, v) if usize::from(s.number() - 1) == slot => {
                    voltage = v.get();
                }
                Write::PresetCurrent(s, a) if usize::from(s.number() - 1) == slot => {
                    current = a.get();
                }
                _ => {}
            }
        }
        Some((voltage, current))
    }

    /// A protection limit as shown.
    pub fn limit(&self, limit: Limit) -> Option<u32> {
        let dump = &self.snapshot.as_ref()?.dump;
        self.pending
            .iter()
            .find(|write| write.register() == limit.write(0).register())
            .map(|write| write.thousandths())
            .or_else(|| Some(limit.value(dump)))
    }

    /// Slot whose V and I equal the setpoints exactly.
    pub fn matching_preset(&self) -> Option<usize> {
        let (v, i) = (self.set_voltage()?.get(), self.set_current()?.get());
        (0..6).find(|slot| self.preset(*slot) == Some((v, i)))
    }

    /// Why slot `slot` cannot be recalled: above the live maximum, or breaking
    /// an ordering rule (PRE-02). `None` when it can.
    pub fn preset_blocked(&self, slot: usize) -> Option<Blocked> {
        let (voltage, current) = self.preset(slot)?;
        self.values_blocked(voltage, current)
    }

    fn values_blocked(&self, voltage: u32, current: u32) -> Option<Blocked> {
        let dump = &self.snapshot.as_ref()?.dump;
        let bounds = Bounds::new(dump, voltage, current);
        let (v, a) = (two_decimals(voltage), amps3(current));
        let blocked = |standard: String, compact: String| Some(Blocked { standard, compact });
        if let Err(reject) = bounds.check_voltage(voltage) {
            return match reject {
                Reject::AboveLiveMax { max, .. } => {
                    let max = two_decimals(max);
                    blocked(
                        format!("{v} V exceeds the live maximum {max} V"),
                        format!("above {max} V"),
                    )
                }
                Reject::NotBelowOvp { ovp } => {
                    let ovp = two_decimals(ovp);
                    blocked(
                        format!("{v} V is not below OVP {ovp} V"),
                        format!("not below OVP {ovp} V"),
                    )
                }
                Reject::NotBelowOpp { power, opp } => {
                    let (power, opp) = (limits::one_decimal(power), limits::one_decimal(opp));
                    blocked(
                        format!("{power} W is not below OPP {opp} W"),
                        format!("not below OPP {opp} W"),
                    )
                }
                Reject::NotBelowOcp { .. } => None,
            };
        }
        if let Err(reject) = bounds.check_current(current) {
            return match reject {
                Reject::AboveLiveMax { max, .. } => {
                    let max = two_decimals(max);
                    blocked(
                        format!("{a} A exceeds the live maximum {max} A", a = a),
                        format!("above {max} A"),
                    )
                }
                Reject::NotBelowOcp { ocp } => {
                    let ocp = two_decimals(ocp);
                    blocked(
                        format!("{a} A is not below OCP {ocp} A"),
                        format!("not below OCP {ocp} A"),
                    )
                }
                Reject::NotBelowOpp { power, opp } => {
                    let (power, opp) = (limits::one_decimal(power), limits::one_decimal(opp));
                    blocked(
                        format!("{power} W is not below OPP {opp} W"),
                        format!("not below OPP {opp} W"),
                    )
                }
                Reject::NotBelowOvp { .. } => None,
            };
        }
        None
    }

    /// The validation line of the edit box: `Ok(text)` for `✓`, `Err(text)`
    /// for `!` (a value that would be blocked on recall; still writable).
    pub fn preset_validation(&self, voltage: u32, current: u32) -> Result<String, String> {
        let Some(dump) = self.snapshot.as_ref().map(|s| &s.dump) else {
            return Ok(String::new());
        };
        match self.values_blocked(voltage, current) {
            Some(blocked) => Err(format!("{}. Recall is blocked.", blocked.standard)),
            None => Ok(format!(
                "within {} V / {} A",
                two_decimals(dump.max_voltage.get()),
                two_decimals(dump.max_current.get())
            )),
        }
    }

    /// The protection context: setpoints as shown, input and temperature.
    pub fn protection_context(&self) -> Option<protection::Context> {
        let dump = &self.snapshot.as_ref()?.dump;
        let milli = |x: f32| (f64::from(x.max(0.0)) * 1000.0).round() as u32;
        Some(protection::Context {
            set_voltage: self.set_voltage()?.get(),
            set_current: self.set_current()?.get(),
            vin: milli(dump.input_voltage),
            temperature: milli(dump.temperature),
        })
    }

    /// Names to write to `presets.toml`, if they changed since last asked.
    pub fn take_names_to_save(&mut self) -> Option<crate::names::Names> {
        std::mem::take(&mut self.names_dirty).then(|| self.names.clone())
    }

    /// The question waiting for an answer, in both forms (§10).
    pub fn prompt_text(&self, compact: bool) -> Option<String> {
        let slot_text = |slot: usize| format!("M{}", slot + 1);
        if let Some(text) = self.output_prompt_text(self.prompt.as_ref()?, compact) {
            return Some(text);
        }
        Some(match self.prompt.as_ref()? {
            Prompt::Overwrite {
                slot,
                voltage,
                current,
            } => {
                if compact {
                    format!("Overwrite {}?", slot_text(*slot))
                } else {
                    let (old_v, old_i) = self.preset(*slot).unwrap_or(CLEARED);
                    format!(
                        "Overwrite {}?  {} V/{} A → {} V/{} A",
                        slot_text(*slot),
                        two_decimals(old_v),
                        amps3(old_i),
                        two_decimals(*voltage),
                        amps3(*current)
                    )
                }
            }
            Prompt::Clear { slot } => {
                if compact {
                    format!("Clear {}?", slot_text(*slot))
                } else {
                    let (v, i) = self.preset(*slot).unwrap_or(CLEARED);
                    let name = &self.names[*slot];
                    let named = if name.is_empty() {
                        String::new()
                    } else {
                        format!(" \"{name}\"")
                    };
                    format!(
                        "Clear {}{named} ({} V / {} A)?",
                        slot_text(*slot),
                        two_decimals(v),
                        amps3(i)
                    )
                }
            }
            // The output prompts were answered above.
            _ => return None,
        })
    }

    // ---- keys: dispatch -------------------------------------------------------

    /// Keys on Presets and Protection, and the prompts and the recall chord.
    pub(crate) fn secondary_key(&mut self, key: Key) -> Effect {
        if self.prompt.is_some() {
            return self.prompt_key(key);
        }
        if self.chord {
            return self.chord_key(key);
        }
        match self.screen {
            Screen::Presets => self.presets_key(key),
            Screen::Protection => self.protection_key(key),
            Screen::Main => Effect::None,
        }
    }

    pub(crate) fn open_screen(&mut self, screen: Screen) {
        self.screen = screen;
        self.chord = false;
        match screen {
            Screen::Presets => self.presets = PresetsState::default(),
            Screen::Protection => self.protection = ProtectionState::default(),
            Screen::Main => {}
        }
    }

    /// `m` on the main panel: the next digit recalls (EDT-07).
    pub(crate) fn start_chord(&mut self) {
        if self.editable() {
            self.chord = true;
        }
    }

    fn chord_key(&mut self, key: Key) -> Effect {
        self.chord = false;
        match key {
            Key::Char(c @ '1'..='6') => self.recall(usize::from(c as u8 - b'1')),
            // Space and Ctrl-X cancel the chord and turn the output OFF.
            Key::Char(' ') | Key::CtrlX => self.off_only(),
            _ => Effect::None,
        }
    }

    // ---- recall ----------------------------------------------------------------

    /// Writes the slot's V and I as the setpoints; never touches the output
    /// (PRE-01). A blocked slot is refused, never clamped (PRE-02).
    fn recall(&mut self, slot: usize) -> Effect {
        if !self.editable() {
            return Effect::None;
        }
        let Some((voltage, current)) = self.preset(slot) else {
            return Effect::None;
        };
        let name = format!("M{}", slot + 1);
        if let Some(blocked) = self.preset_blocked(slot) {
            self.set_notice(
                Text {
                    standard: format!("{name} blocked: {}", blocked.standard),
                    compact: format!("{name} blocked: {}", blocked.compact),
                },
                Tone::Warn,
                Some(self.now + crate::constants::NOTICE_DURATION),
            );
            return Effect::None;
        }
        if self.output_is_on() {
            self.recall_while_on(slot, voltage, current);
            return Effect::None;
        }
        let writes = vec![
            Write::SetVoltage(Millivolts::new(voltage)),
            Write::SetCurrent(Milliamps::new(current)),
        ];
        for write in &writes {
            self.stage_write(*write);
        }
        let (v, a) = (two_decimals(voltage), amps3(current));
        self.set_notice(
            Text {
                standard: format!("Recalled {name} · {v} V / {a} A"),
                compact: format!("Recalled {name} · {v}V/{a}A"),
            },
            Tone::Warn,
            Some(self.now + crate::constants::NOTICE_DURATION),
        );
        Effect::Writes(writes)
    }

    // ---- Presets ----------------------------------------------------------------

    fn presets_key(&mut self, key: Key) -> Effect {
        match self.presets.mode.clone() {
            PresetMode::List => self.presets_list_key(key),
            PresetMode::Edit(edit) => self.presets_edit_key(key, edit),
            PresetMode::Rename(text) => self.presets_rename_key(key, text),
        }
    }

    fn presets_list_key(&mut self, key: Key) -> Effect {
        let slot = self.presets.selected;
        match key {
            Key::Up => self.presets.selected = slot.saturating_sub(1),
            Key::Down => self.presets.selected = (slot + 1).min(5),
            Key::Esc | Key::Char('q') => self.screen = Screen::Main,
            Key::CtrlC => return Effect::Quit,
            Key::Enter => return self.recall(slot),
            Key::Char('e') => {
                if self.editable()
                    && let Some((voltage, current)) = self.preset(slot)
                {
                    self.presets.mode = PresetMode::Edit(PresetEdit {
                        voltage,
                        current,
                        focus: Focus::Voltage,
                        cursors: [1, 1],
                    });
                }
            }
            Key::Char('n') => {
                self.presets.mode = PresetMode::Rename(self.names[slot].clone());
            }
            Key::Char('s') => self.ask_overwrite(slot),
            Key::Char('x') | Key::Char('X') => self.ask_clear(slot),
            _ => {}
        }
        Effect::None
    }

    fn presets_edit_key(&mut self, key: Key, mut edit: PresetEdit) -> Effect {
        let slot = self.presets.selected;
        let quantity = match edit.focus {
            Focus::Voltage => limits::Quantity::Voltage,
            Focus::Current => limits::Quantity::Current,
        };
        let index = match edit.focus {
            Focus::Voltage => 0,
            Focus::Current => 1,
        };
        let dump_max = self
            .snapshot
            .as_ref()
            .map(|s| (s.dump.ovp_range.get(), s.dump.max_current.get()));
        match key {
            Key::Esc | Key::Char('q') => {
                self.presets.mode = PresetMode::List;
                return Effect::None;
            }
            Key::CtrlC => return Effect::Quit,
            Key::Tab => {
                edit.focus = match edit.focus {
                    Focus::Voltage => Focus::Current,
                    Focus::Current => Focus::Voltage,
                };
            }
            Key::Left => edit.cursors[index] = edit.cursors[index].saturating_sub(1),
            Key::Right => edit.cursors[index] = (edit.cursors[index] + 1).min(3),
            Key::Up | Key::Down => {
                let weight = limits::digit_weight(quantity, edit.cursors[index]);
                let Some((v_max, i_max)) = dump_max else {
                    return Effect::None;
                };
                let (value, max) = match edit.focus {
                    Focus::Voltage => (&mut edit.voltage, v_max),
                    Focus::Current => (&mut edit.current, i_max),
                };
                *value = if key == Key::Up {
                    value.saturating_add(weight).min(max).max(*value)
                } else {
                    value.saturating_sub(weight)
                };
            }
            Key::Enter => {
                if !self.editable() {
                    return Effect::None;
                }
                self.presets.mode = PresetMode::List;
                return self.write_slot(slot, edit.voltage, edit.current);
            }
            _ => {}
        }
        self.presets.mode = PresetMode::Edit(edit);
        Effect::None
    }

    fn presets_rename_key(&mut self, key: Key, mut text: String) -> Effect {
        let slot = self.presets.selected;
        match key {
            Key::Esc => {
                self.presets.mode = PresetMode::List;
                return Effect::None;
            }
            Key::CtrlC => return Effect::Quit,
            Key::Enter => {
                self.names[slot] = text.trim_end().to_string();
                self.presets.mode = PresetMode::List;
                self.names_changed();
                return Effect::None;
            }
            Key::Backspace => {
                text.pop();
            }
            // Space types a space here; Ctrl-X is for the output (§1.5).
            Key::Char(c) if !c.is_control() && text.chars().count() < MAX_NAME => text.push(c),
            _ => {}
        }
        self.presets.mode = PresetMode::Rename(text);
        Effect::None
    }

    fn names_changed(&mut self) {
        if self.names_writable {
            self.names_dirty = true;
        } else {
            self.same_notice(
                "Names not saved: presets.toml is invalid",
                Tone::Warn,
                Some(self.now + crate::constants::NOTICE_DURATION),
            );
        }
    }

    fn write_slot(&mut self, slot: usize, voltage: u32, current: u32) -> Effect {
        let number = protocol::Slot::new(slot as u8 + 1).expect("slot 0..6");
        let writes = vec![
            Write::PresetVoltage(number, Millivolts::new(voltage)),
            Write::PresetCurrent(number, Milliamps::new(current)),
        ];
        for write in &writes {
            self.stage_write(*write);
        }
        Effect::Writes(writes)
    }

    /// `s`: save the setpoints into the slot, asking first unless it already
    /// holds exactly them (SAF-12).
    fn ask_overwrite(&mut self, slot: usize) {
        if !self.editable() {
            return;
        }
        let (Some(voltage), Some(current)) = (self.set_voltage(), self.set_current()) else {
            return;
        };
        let (voltage, current) = (voltage.get(), current.get());
        if self.preset(slot) == Some((voltage, current)) {
            self.same_notice(
                &format!("M{} already holds these values", slot + 1),
                Tone::Warn,
                Some(self.now + crate::constants::NOTICE_DURATION),
            );
            return;
        }
        self.prompt = Some(Prompt::Overwrite {
            slot,
            voltage,
            current,
        });
    }

    /// `x`: reset the slot to 5.00 V / 1.000 A and drop its name, asking
    /// first unless it is already so (SAF-12).
    fn ask_clear(&mut self, slot: usize) {
        if !self.editable() {
            return;
        }
        if self.preset(slot) == Some(CLEARED) && self.names[slot].is_empty() {
            self.same_notice(
                &format!("M{} is already clear", slot + 1),
                Tone::Warn,
                Some(self.now + crate::constants::NOTICE_DURATION),
            );
            return;
        }
        self.prompt = Some(Prompt::Clear { slot });
    }

    /// `y` yes, `n` or `Esc` no. `⏎` is ignored so the `⏎` that started the
    /// action cannot also confirm it (§5.12).
    fn prompt_key(&mut self, key: Key) -> Effect {
        let Some(prompt) = self.prompt.clone() else {
            return Effect::None;
        };
        match key {
            Key::Char('y') | Key::Char('Y') => {
                self.prompt = None;
                match prompt {
                    Prompt::Overwrite {
                        slot,
                        voltage,
                        current,
                    } => self.write_slot(slot, voltage, current),
                    Prompt::Clear { slot } => {
                        self.names[slot].clear();
                        self.names_changed();
                        self.write_slot(slot, CLEARED.0, CLEARED.1)
                    }
                    other => self.confirm_output_prompt(&other).unwrap_or(Effect::None),
                }
            }
            Key::Char('n') | Key::Char('N') | Key::Esc => {
                self.prompt = None;
                Effect::None
            }
            // Space and Ctrl-X cancel the question and turn the output OFF if
            // it is ON; they never turn it ON (SAF-05).
            Key::Char(' ') | Key::CtrlX => {
                self.prompt = None;
                self.off_only()
            }
            Key::CtrlC => Effect::Quit,
            _ => Effect::None,
        }
    }

    // ---- Protection ---------------------------------------------------------------

    fn selected_limit(&self) -> Limit {
        ALL[self.protection.selected.min(4)]
    }

    fn protection_key(&mut self, key: Key) -> Effect {
        match self.protection.mode.clone() {
            ProtectionMode::Selecting => self.protection_select_key(key),
            ProtectionMode::Editing(edit) => self.protection_edit_key(key, edit),
        }
    }

    fn start_editing(&mut self, candidate: u32, from_default: bool, typed: Option<String>) {
        if !self.editable() {
            return;
        }
        let limit = self.selected_limit();
        self.protection.mode = ProtectionMode::Editing(ProtectionEdit {
            candidate,
            cursor: limit.initial_digit(),
            typed: typed.map(|text| Typed {
                text,
                rejected: false,
            }),
            from_default,
        });
    }

    fn protection_select_key(&mut self, key: Key) -> Effect {
        let limit = self.selected_limit();
        match key {
            Key::Up => self.protection.selected = self.protection.selected.saturating_sub(1),
            Key::Down => self.protection.selected = (self.protection.selected + 1).min(4),
            Key::Esc | Key::Char('q') => self.screen = Screen::Main,
            Key::CtrlC => return Effect::Quit,
            Key::Enter => {
                if let Some(value) = self.limit(limit) {
                    self.start_editing(value, false, None);
                }
            }
            Key::Char('r') => self.start_editing(limit.default_value(), true, None),
            Key::Char(c) if c.is_ascii_digit() || c == '.' => {
                if let Some(value) = self.limit(limit) {
                    self.start_editing(value, false, Some(c.to_string()));
                }
            }
            _ => {}
        }
        Effect::None
    }

    /// Is the candidate valid? `⏎` is disabled while it is not (§8.3).
    pub fn protection_check(&self, limit: Limit, candidate: u32) -> Option<protection::Check> {
        let dump = &self.snapshot.as_ref()?.dump;
        Some(protection::check(
            limit,
            candidate,
            dump,
            &self.protection_context()?,
        ))
    }

    fn protection_edit_key(&mut self, key: Key, mut edit: ProtectionEdit) -> Effect {
        let limit = self.selected_limit();
        // Typed mode: digits, one point, Backspace, Enter, Esc.
        if let Some(typed) = edit.typed.as_mut() {
            typed.rejected = false;
            match key {
                Key::Esc => {
                    self.protection.mode = ProtectionMode::Selecting;
                    return Effect::None;
                }
                Key::Char(' ') | Key::CtrlX => {
                    self.protection.mode = ProtectionMode::Selecting;
                    return self.off_only();
                }
                Key::CtrlC => return Effect::Quit,
                Key::Char(c) if c.is_ascii_digit() => {
                    if typed.text.chars().count() < crate::constants::ENTRY_MAX_CHARS {
                        typed.text.push(c);
                    }
                }
                Key::Char('.') => {
                    if typed.text.chars().count() < crate::constants::ENTRY_MAX_CHARS
                        && !typed.text.contains('.')
                    {
                        typed.text.push('.');
                    }
                }
                Key::Backspace => {
                    typed.text.pop();
                }
                Key::Enter => return self.apply_typed_limit(limit, edit),
                _ => {}
            }
            self.protection.mode = ProtectionMode::Editing(edit);
            return Effect::None;
        }
        match key {
            Key::Esc | Key::Char('q') => {
                self.protection.mode = ProtectionMode::Selecting;
                return Effect::None;
            }
            Key::CtrlC => return Effect::Quit,
            Key::Left => edit.cursor = edit.cursor.saturating_sub(1),
            Key::Right => edit.cursor = (edit.cursor + 1).min(limit.weights().len() - 1),
            Key::Up | Key::Down => {
                let weight = limit.weights()[edit.cursor.min(limit.weights().len() - 1)];
                let max = self
                    .snapshot
                    .as_ref()
                    .map_or(u32::MAX, |s| limit.range_max(&s.dump));
                edit.candidate = if key == Key::Up {
                    edit.candidate
                        .saturating_add(weight)
                        .min(max)
                        .max(edit.candidate)
                } else {
                    edit.candidate.saturating_sub(weight)
                };
                edit.from_default = false;
            }
            Key::Char('r') => {
                edit.candidate = limit.default_value();
                edit.from_default = true;
            }
            Key::Char(c) if c.is_ascii_digit() || c == '.' => {
                edit.typed = Some(Typed {
                    text: c.to_string(),
                    rejected: false,
                });
            }
            Key::Enter => {
                // Disabled while the candidate is invalid.
                let valid = self
                    .protection_check(limit, edit.candidate)
                    .is_some_and(|check| check.ok);
                if !valid || !self.editable() {
                    return Effect::None;
                }
                self.protection.mode = ProtectionMode::Selecting;
                return self.write_limit(limit, edit.candidate);
            }
            _ => {}
        }
        self.protection.mode = ProtectionMode::Editing(edit);
        Effect::None
    }

    fn write_limit(&mut self, limit: Limit, value: u32) -> Effect {
        if self.limit(limit) == Some(value) {
            return Effect::None;
        }
        let write = limit.write(value);
        self.stage_write(write);
        Effect::Write(write)
    }

    fn apply_typed_limit(&mut self, limit: Limit, mut edit: ProtectionEdit) -> Effect {
        let text = edit
            .typed
            .as_ref()
            .map(|t| t.text.clone())
            .unwrap_or_default();
        let step = limit.weights().last().copied().unwrap_or(10);
        let value = match limits::parse_decimal(&text, step) {
            Parsed::Empty => {
                self.protection.mode = ProtectionMode::Selecting;
                return Effect::None;
            }
            Parsed::NotANumber => {
                return self.reject_typed(edit, format!("{text} is not a number"));
            }
            Parsed::Value(value) => value,
        };
        let Some(check) = self.protection_check(limit, value) else {
            return Effect::None;
        };
        if !check.ok {
            let unit = limit.text(0).rsplit(' ').next().unwrap_or("").to_string();
            return self.reject_typed(edit, format!("{text} {unit}: {}", check.compact));
        }
        if !self.editable() {
            return Effect::None;
        }
        edit.typed = None;
        self.protection.mode = ProtectionMode::Selecting;
        self.write_limit(limit, value)
    }

    fn reject_typed(&mut self, mut edit: ProtectionEdit, message: String) -> Effect {
        if let Some(typed) = edit.typed.as_mut() {
            typed.rejected = true;
        }
        self.protection.mode = ProtectionMode::Editing(edit);
        self.same_notice(&message, Tone::Error, None);
        Effect::None
    }
}
