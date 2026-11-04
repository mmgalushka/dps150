//! The model the main panel draws (APP.md §2, §7): one snapshot owned by the
//! app thread, updated from device messages and key presses.
//!
//! Setpoint edits are optimistic (EDT-04): the value the user chose shows at
//! once and stays until the next dump has read it back; if the device holds
//! something else, the device value replaces it with a notice (PRT-03).

use std::time::Duration;

use device::{Event, Message, Refusal, Snapshot};
use protocol::Write;
use protocol::value::{Milliamps, Millivolts};

use crate::constants::{ENTRY_MAX_CHARS, NOTICE_DURATION};
use crate::limits::{self, Bounds, Parsed, Quantity, Text};
use crate::names::Names;
use crate::output::Safety;
use crate::screens::{PresetsState, Prompt, ProtectionState, Screen};

/// Which readout has the focus (EDT-01).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Voltage,
    Current,
}

impl Focus {
    fn quantity(self) -> Quantity {
        match self {
            Self::Voltage => Quantity::Voltage,
            Self::Current => Quantity::Current,
        }
    }
}

/// A key press, reduced to what the app understands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Left,
    Right,
    Up,
    Down,
    Tab,
    Enter,
    Esc,
    Backspace,
    CtrlC,
    CtrlX,
}

/// What the caller must do after a key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Effect {
    None,
    /// Quit by the rules of Settings → On exit (SAF-11).
    Quit,
    /// Turn the output off, then quit (`o` at the quit prompt).
    QuitOff,
    /// One reconnect attempt (`r`, Reconnect = off).
    Reconnect,
    /// Send this write to the device thread.
    Write(Write),
    /// Send these writes in order (a recall, a slot).
    Writes(Vec<Write>),
}

/// What the screens show of the link (LNK-02).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkStatus {
    Online,
    /// Down. `auto_reconnect` picks the wording of the message.
    Lost {
        auto_reconnect: bool,
    },
}

/// How a message line is styled: hints and warnings are `wn`, errors `er`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    Warn,
    Error,
}

/// The text for the message line (§5.12).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub text: String,
    pub tone: Tone,
}

/// Typed entry in progress (EDT-05, EDT-06).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub focus: Focus,
    pub text: String,
    /// The last apply was refused: the text turns `er`.
    pub rejected: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Notice {
    text: Text,
    tone: Tone,
    /// `None`: stays until the next key press.
    expires: Option<Duration>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    pub link: LinkStatus,
    /// The last dump with pushed values merged in. Stale while the link is down.
    pub snapshot: Option<Snapshot>,
    /// Port path, shown in the header.
    pub port: String,
    pub focus: Focus,
    voltage_cursor: usize,
    current_cursor: usize,
    pub(crate) entry: Option<Entry>,
    pub(crate) notice: Option<Notice>,
    /// Values chosen by the user and not yet read back.
    pub(crate) pending: Vec<Write>,
    pub(crate) now: Duration,
    reconnect: bool,
    pub(crate) screen: Screen,
    pub(crate) presets: PresetsState,
    pub(crate) protection: ProtectionState,
    pub(crate) names: Names,
    pub(crate) names_writable: bool,
    pub(crate) names_dirty: bool,
    pub(crate) prompt: Option<Prompt>,
    pub(crate) chord: bool,
    pub(crate) safety: Safety,
    /// The setpoint at the last output ON or confirmed prompt, which the
    /// jump guard measures from (SAFETY.md §2).
    pub(crate) reference: Option<(u32, u32)>,
    /// An OFF waits for the link to come back (LNK-04).
    pub(crate) off_queued: bool,
    pub(crate) too_small: bool,
    /// Writes no key caused (the queued OFF after a reconnect).
    pub(crate) outgoing: Vec<Write>,
    /// The help overlay is open, at this page (Compact scrolls by page).
    pub(crate) help: Option<usize>,
}

impl Model {
    /// `reconnect` is Settings → Reconnect.
    pub fn new(port: impl Into<String>, reconnect: bool) -> Self {
        Self {
            link: LinkStatus::Online,
            snapshot: None,
            port: port.into(),
            focus: Focus::Voltage,
            // Initial cursor: voltage units digit, current tenths (EDT-01).
            voltage_cursor: 1,
            current_cursor: 1,
            entry: None,
            notice: None,
            pending: Vec::new(),
            now: Duration::ZERO,
            reconnect,
            screen: Screen::Main,
            presets: PresetsState::default(),
            protection: ProtectionState::default(),
            names: Default::default(),
            names_writable: true,
            names_dirty: false,
            prompt: None,
            chord: false,
            safety: Safety::default(),
            reference: None,
            off_queued: false,
            too_small: false,
            outgoing: Vec::new(),
            help: None,
        }
    }

    /// The slot names read from `presets.toml`; `writable` is false when the
    /// file is invalid and must not be overwritten (APP-05).
    pub fn set_names(&mut self, names: Names, writable: bool) {
        self.names = names;
        self.names_writable = writable;
    }

    pub fn set_reconnect(&mut self, on: bool) {
        self.reconnect = on;
        if let LinkStatus::Lost { .. } = self.link {
            self.link = LinkStatus::Lost { auto_reconnect: on };
        }
    }

    /// A notice that disappears after 3 s or on the next key press.
    pub fn notify(&mut self, text: impl Into<String>) {
        let text = text.into();
        self.set_notice(
            Text {
                standard: text.clone(),
                compact: text,
            },
            Tone::Warn,
            Some(self.now + NOTICE_DURATION),
        );
    }

    pub(crate) fn set_notice(&mut self, text: Text, tone: Tone, expires: Option<Duration>) {
        self.notice = Some(Notice {
            text,
            tone,
            expires,
        });
    }

    pub(crate) fn same_notice(&mut self, text: &str, tone: Tone, expires: Option<Duration>) {
        self.set_notice(
            Text {
                standard: text.to_string(),
                compact: text.to_string(),
            },
            tone,
            expires,
        );
    }

    /// Moves the clock; an expired notice goes away.
    pub fn tick(&mut self, now: Duration) {
        self.now = now;
        if self
            .notice
            .as_ref()
            .and_then(|notice| notice.expires)
            .is_some_and(|at| now >= at)
        {
            self.notice = None;
        }
    }

    pub fn apply(&mut self, message: Message) {
        match message {
            Message::Model(snapshot) => {
                self.snapshot = Some(*snapshot);
                self.follow_reference();
            }
            Message::Event(Event::Online { restored }) => {
                self.link = LinkStatus::Online;
                if restored {
                    self.link_restored();
                }
            }
            Message::Event(Event::Lost(_) | Event::Failed(_)) => {
                self.link = LinkStatus::Lost {
                    auto_reconnect: self.reconnect,
                };
                // Typed entry and unconfirmed values end with the link
                // (LNK-05, LNK-03).
                self.entry = None;
                self.pending.clear();
                self.notice = None;
                self.reference = None;
                self.prompt = None;
                self.chord = false;
                if matches!(self.presets.mode, crate::screens::PresetMode::Edit(_)) {
                    self.presets.mode = crate::screens::PresetMode::List;
                }
                self.protection.mode = crate::screens::ProtectionMode::Selecting;
            }
            Message::Event(Event::ReadBack { written, device }) => self.read_back(written, device),
            // The close is the quit sequence's business, not the screens'.
            Message::Event(Event::Closed) => {}
            Message::Event(Event::WriteRefused { why, .. }) => {
                self.pending.clear();
                match why {
                    Refusal::NotOnline => self.link_down_notice(),
                    Refusal::OffPending => self.same_notice(
                        "Output OFF is still pending",
                        Tone::Warn,
                        Some(self.now + NOTICE_DURATION),
                    ),
                    Refusal::OutOfRange => self.same_notice(
                        "Write refused: outside the device range",
                        Tone::Error,
                        Some(self.now + NOTICE_DURATION),
                    ),
                }
            }
        }
    }

    /// The dump read after a SET has arrived (PRT-03).
    fn read_back(&mut self, written: Write, device: Write) {
        self.pending.retain(|pending| {
            !(pending.register() == written.register()
                && pending.thousandths() == written.thousandths())
        });
        if written == Write::Output(false) && device == Write::Output(true) {
            // The one read-back that must never pass quietly.
            self.set_notice(
                Text {
                    standard: "Output did not turn OFF. Use the front-panel button".into(),
                    compact: "OFF failed: use the panel button".into(),
                },
                Tone::Error,
                None,
            );
        } else if written.differs_from(device) {
            let text = format!("Device kept {}", describe(device));
            self.same_notice(&text, Tone::Warn, Some(self.now + NOTICE_DURATION));
        }
    }

    // ---- what the screens read ------------------------------------------------

    pub fn entry(&self) -> Option<&Entry> {
        self.entry.as_ref()
    }

    /// The voltage setpoint as shown: the pending value, else the device's.
    pub fn set_voltage(&self) -> Option<Millivolts> {
        self.pending
            .iter()
            .find_map(|write| match write {
                Write::SetVoltage(value) => Some(*value),
                _ => None,
            })
            .or_else(|| self.snapshot.as_ref().map(|s| s.dump.set_voltage))
    }

    pub fn set_current(&self) -> Option<Milliamps> {
        self.pending
            .iter()
            .find_map(|write| match write {
                Write::SetCurrent(value) => Some(*value),
                _ => None,
            })
            .or_else(|| self.snapshot.as_ref().map(|s| s.dump.set_current))
    }

    /// Digit index of `focus`: 0..=3, counting digits only (skips the `.`).
    pub fn cursor(&self, focus: Focus) -> usize {
        match focus {
            Focus::Voltage => self.voltage_cursor,
            Focus::Current => self.current_cursor,
        }
    }

    pub fn is_online(&self) -> bool {
        self.link == LinkStatus::Online
    }

    /// The text for the message line, if any: a rejection or notice, the
    /// typed-entry hint, or the persistent live-maximum warning (§5.12, LIM-07).
    pub fn message(&self, compact: bool) -> Option<Line> {
        let pick = |text: &Text| {
            if compact {
                text.compact.clone()
            } else {
                text.standard.clone()
            }
        };
        if let Some(notice) = &self.notice {
            return Some(Line {
                text: pick(&notice.text),
                tone: notice.tone,
            });
        }
        if self.chord {
            let text = if compact {
                "Recall which? 1-6, Esc"
            } else {
                "Recall which preset? 1-6, Esc cancels"
            };
            return Some(Line {
                text: text.into(),
                tone: Tone::Warn,
            });
        }
        let dump = &self.snapshot.as_ref()?.dump;
        if let Some(entry) = &self.entry {
            let text = match (entry.focus, compact) {
                (Focus::Voltage, false) => format!(
                    "Typing a voltage. Range 0 – {} V (live maximum)",
                    limits::two_decimals(dump.max_voltage.get())
                ),
                (Focus::Current, false) => format!(
                    "Typing a current. Range 0 – {} A",
                    limits::two_decimals(dump.max_current.get())
                ),
                (Focus::Voltage, true) => format!(
                    "Voltage 0 – {} V (live max)",
                    limits::two_decimals(dump.max_voltage.get())
                ),
                (Focus::Current, true) => format!(
                    "Current 0 – {} A (live max)",
                    limits::two_decimals(dump.max_current.get())
                ),
            };
            return Some(Line {
                text,
                tone: Tone::Warn,
            });
        }
        if !self.is_online() {
            return None;
        }
        let (set_v, set_i) = (self.set_voltage()?.get(), self.set_current()?.get());
        let text = if set_v > dump.max_voltage.get() {
            let (v, max) = (
                limits::two_decimals(set_v),
                limits::two_decimals(dump.max_voltage.get()),
            );
            if compact {
                format!("V {v} above live max {max}")
            } else {
                format!("Set {v} V is above the live maximum {max} V. Lower it to turn output ON")
            }
        } else if set_i > dump.max_current.get() {
            let (i, max) = (
                format!("{}.{:03}", set_i / 1000, set_i % 1000),
                limits::two_decimals(dump.max_current.get()),
            );
            if compact {
                format!("I {i} above live max {max}")
            } else {
                format!("Set {i} A is above the live maximum {max} A. Lower it to turn output ON")
            }
        } else {
            return None;
        };
        Some(Line {
            text,
            tone: Tone::Warn,
        })
    }

    // ---- keys -----------------------------------------------------------------

    pub fn toggle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Voltage => Focus::Current,
            Focus::Current => Focus::Voltage,
        };
    }

    /// `←`: one digit left; at the end it does nothing (EDT-02).
    pub fn cursor_left(&mut self) {
        let cursor = self.focused_cursor();
        *cursor = cursor.saturating_sub(1);
    }

    /// `→`: one digit right; at the end it does nothing.
    pub fn cursor_right(&mut self) {
        let cursor = self.focused_cursor();
        *cursor = (*cursor + 1).min(3);
    }

    fn focused_cursor(&mut self) -> &mut usize {
        match self.focus {
            Focus::Voltage => &mut self.voltage_cursor,
            Focus::Current => &mut self.current_cursor,
        }
    }

    /// Setpoint writes only. `Space`, `Ctrl-X` and the output arrive with
    /// the output milestone; in typed entry they cancel the entry.
    pub fn on_key(&mut self, key: Key) -> Effect {
        // Any key clears a notice or a rejection (§5.12).
        self.notice = None;
        if self.too_small {
            // Only OFF and Ctrl-C work; `q` is ignored (SAF-10).
            return match key {
                Key::CtrlC => Effect::Quit,
                Key::Char(' ') | Key::CtrlX => self.off_only(),
                _ => Effect::None,
            };
        }
        if self.help.is_some() {
            return self.help_key(key);
        }
        if key == Key::Char('?') && self.can_open_help() {
            self.help = Some(0);
            return Effect::None;
        }
        // `Space` and `Ctrl-X` outside the contexts that only cancel (typed
        // entry, prompts, the chord): `Ctrl-X` is OFF in every mode, `Space`
        // the toggle (SAF-01, SAF-02). In the rename field `Space` is a
        // character.
        let cancels =
            self.prompt.is_some() || self.chord || self.entry.is_some() || self.protection_typing();
        match key {
            Key::CtrlX if !cancels => return self.off_only(),
            Key::Char(' ') if !cancels && !self.renaming() => return self.toggle_output(),
            _ => {}
        }
        if self.screen != Screen::Main || self.prompt.is_some() || self.chord {
            return self.secondary_key(key);
        }
        if self.entry.is_some() {
            return self.entry_key(key);
        }
        match key {
            Key::Char('p') => {
                self.open_screen(Screen::Presets);
                Effect::None
            }
            Key::Char('o') => {
                self.open_screen(Screen::Protection);
                Effect::None
            }
            Key::Char('m') => {
                self.start_chord();
                Effect::None
            }
            // `Ctrl-C` quits at once; `q` asks when it would leave the output
            // ON or cannot reach the device (SAF-11).
            Key::CtrlC => Effect::Quit,
            Key::Char('q') => self.request_quit(),
            Key::Tab => {
                self.toggle_focus();
                Effect::None
            }
            Key::Left => {
                self.cursor_left();
                Effect::None
            }
            Key::Right => {
                self.cursor_right();
                Effect::None
            }
            Key::Up => self.step(true),
            Key::Down => self.step(false),
            Key::Enter => {
                self.start_entry(self.focus, String::new());
                Effect::None
            }
            Key::Char('v' | 'V') => {
                self.focus = Focus::Voltage;
                self.start_entry(Focus::Voltage, String::new());
                Effect::None
            }
            Key::Char('i' | 'I') => {
                self.focus = Focus::Current;
                self.start_entry(Focus::Current, String::new());
                Effect::None
            }
            Key::Char(c) if c.is_ascii_digit() || c == '.' => {
                self.start_entry(self.focus, c.to_string());
                Effect::None
            }
            Key::Char('r')
                if self.link
                    == (LinkStatus::Lost {
                        auto_reconnect: false,
                    }) =>
            {
                Effect::Reconnect
            }
            _ => Effect::None,
        }
    }

    fn link_down_notice(&mut self) {
        self.set_notice(
            Text {
                standard: "Link down. Changes are not sent.".into(),
                compact: "Link down: not sent".into(),
            },
            Tone::Warn,
            Some(self.now + NOTICE_DURATION),
        );
    }

    /// Edits need an online link and a dump to start from (LNK-03).
    pub(crate) fn editable(&mut self) -> bool {
        if !self.is_online() {
            self.link_down_notice();
            return false;
        }
        self.snapshot.is_some()
    }

    /// While the output is ON a setpoint change would reach the load at once;
    /// the confirmations that guard that come with the output milestone, so
    /// nothing is sent.
    pub(crate) fn output_is_on(&self) -> bool {
        self.snapshot.as_ref().is_some_and(|s| s.dump.output_on)
    }

    fn bounds(&self) -> Option<Bounds> {
        let dump = &self.snapshot.as_ref()?.dump;
        Some(Bounds::new(
            dump,
            self.set_voltage()?.get(),
            self.set_current()?.get(),
        ))
    }

    fn start_entry(&mut self, focus: Focus, text: String) {
        if self.editable() {
            self.entry = Some(Entry {
                focus,
                text,
                rejected: false,
            });
        }
    }

    /// `↑` / `↓`: add or subtract the digit under the cursor (EDT-03).
    fn step(&mut self, up: bool) -> Effect {
        if !self.editable() {
            return Effect::None;
        }
        let Some(bounds) = self.bounds() else {
            return Effect::None;
        };
        let quantity = self.focus.quantity();
        let weight = limits::digit_weight(quantity, self.cursor(self.focus));
        let step = bounds.step(quantity, weight, up);
        if let Some(notice) = step.notice {
            let text = notice.text_for(quantity);
            self.same_notice(&text, Tone::Warn, Some(self.now + NOTICE_DURATION));
        }
        let current = match quantity {
            Quantity::Voltage => bounds.set_v,
            Quantity::Current => bounds.set_i,
        };
        if step.value == current {
            return Effect::None;
        }
        self.change(quantity, step.value, &bounds)
    }

    /// A setpoint change of one quantity, through the jump guard (SAF-06).
    fn change(&mut self, quantity: Quantity, value: u32, bounds: &Bounds) -> Effect {
        let (write, voltage, current) = match quantity {
            Quantity::Voltage => (
                Write::SetVoltage(Millivolts::new(value)),
                value,
                bounds.set_i,
            ),
            Quantity::Current => (
                Write::SetCurrent(Milliamps::new(value)),
                bounds.set_v,
                value,
            ),
        };
        self.guarded(vec![write], voltage, current)
    }

    /// Shows `write` at once, until the read-back replaces it (EDT-04).
    pub(crate) fn stage_write(&mut self, write: Write) {
        self.pending
            .retain(|pending| pending.register() != write.register());
        self.pending.push(write);
    }

    fn entry_key(&mut self, key: Key) -> Effect {
        let Some(entry) = self.entry.as_mut() else {
            return Effect::None;
        };
        entry.rejected = false;
        match key {
            Key::CtrlC => return Effect::Quit,
            Key::Char(c) if c.is_ascii_digit() => {
                if entry.text.chars().count() < ENTRY_MAX_CHARS {
                    entry.text.push(c);
                }
            }
            Key::Char('.') => {
                if entry.text.chars().count() < ENTRY_MAX_CHARS && !entry.text.contains('.') {
                    entry.text.push('.');
                }
            }
            Key::Backspace => {
                entry.text.pop();
            }
            Key::Esc => self.entry = None,
            Key::Char(' ') | Key::CtrlX => {
                self.entry = None;
                return self.off_only();
            }
            Key::Enter => return self.apply_entry(),
            // Everything else is ignored, including `v` and `i`.
            _ => {}
        }
        Effect::None
    }

    fn apply_entry(&mut self) -> Effect {
        let Some(entry) = self.entry.clone() else {
            return Effect::None;
        };
        let quantity = entry.focus.quantity();
        let value = match limits::parse_typed(&entry.text, quantity) {
            Parsed::Empty => {
                self.entry = None;
                return Effect::None;
            }
            Parsed::NotANumber => {
                self.reject(Text {
                    standard: format!("{} is not a number", entry.text),
                    compact: format!("{} is not a number", entry.text),
                });
                return Effect::None;
            }
            Parsed::Value(value) => value,
        };
        let Some(bounds) = self.bounds() else {
            return Effect::None;
        };
        let checked = match quantity {
            Quantity::Voltage => bounds.check_voltage(value),
            Quantity::Current => bounds.check_current(value),
        };
        if let Err(reject) = checked {
            self.reject(reject.text(&entry.text));
            return Effect::None;
        }
        self.entry = None;
        let current = match quantity {
            Quantity::Voltage => bounds.set_v,
            Quantity::Current => bounds.set_i,
        };
        if value == current {
            return Effect::None;
        }
        self.change(quantity, value, &bounds)
    }

    /// The entry stays open, its text turns `er`, and the reason stays until
    /// the next key press (§7.2).
    fn reject(&mut self, text: Text) {
        if let Some(entry) = self.entry.as_mut() {
            entry.rejected = true;
        }
        self.set_notice(text, Tone::Error, None);
    }
}

impl Model {
    /// `?` opens the help on every screen, except where it is a typed
    /// character or a question is waiting (§5.13).
    fn can_open_help(&self) -> bool {
        !(self.prompt.is_some()
            || self.chord
            || self.entry.is_some()
            || self.renaming()
            || self.protection_typing())
    }

    /// Keys while the overlay is open: `?`, `Esc` and `q` close it, `↑` `↓`
    /// turn the pages of the Compact overlay; `Space` and `Ctrl-X` still
    /// switch the output OFF, and `Space` never turns it ON because the
    /// question could not be seen under the overlay (§5.13, SAF-01).
    fn help_key(&mut self, key: Key) -> Effect {
        match key {
            Key::Char('?') | Key::Esc | Key::Char('q') => self.help = None,
            Key::Up => self.help = self.help.map(|page| page.saturating_sub(1)),
            Key::Down => {
                let last = crate::help::compact_pages(self.screen) - 1;
                self.help = self.help.map(|page| (page + 1).min(last));
            }
            Key::Char(' ') | Key::CtrlX => return self.off_only(),
            Key::CtrlC => return Effect::Quit,
            _ => {}
        }
        Effect::None
    }

    /// The overlay's page when it is open (0-based).
    pub fn help_page(&self) -> Option<usize> {
        self.help
    }

    fn renaming(&self) -> bool {
        self.screen == Screen::Presets
            && matches!(self.presets.mode, crate::screens::PresetMode::Rename(_))
    }

    fn protection_typing(&self) -> bool {
        self.screen == Screen::Protection
            && matches!(
                &self.protection.mode,
                crate::screens::ProtectionMode::Editing(edit) if edit.typed.is_some()
            )
    }

    /// The link is back with the device's own values on screen. A queued OFF
    /// is the first write (LNK-04, LNK-07).
    fn link_restored(&mut self) {
        let Some(dump) = self.snapshot.as_ref().map(|s| s.dump.clone()) else {
            return;
        };
        let (v, i) = (
            limits::two_decimals(dump.set_voltage.get()),
            format!(
                "{}.{:03}",
                dump.set_current.get() / 1000,
                dump.set_current.get() % 1000
            ),
        );
        let text = if self.off_queued {
            self.off_queued = false;
            self.outgoing.push(Write::Output(false));
            Text {
                standard: "Link restored · queued OFF sent, output OFF".into(),
                compact: "Restored · OFF sent".into(),
            }
        } else if dump.output_on {
            Text {
                standard: format!("Link restored · output ON at {v} V / {i} A"),
                compact: format!("Restored · ON {v} V / {i} A"),
            }
        } else {
            Text {
                standard: "Link restored · output OFF".into(),
                compact: "Restored · output OFF".into(),
            }
        };
        self.set_notice(text, Tone::Warn, Some(self.now + NOTICE_DURATION));
    }
}

/// `5.00 V`, `1.500 A`, `150.0 W`, `80.0 °C` for a write.
fn describe(write: Write) -> String {
    let milli = write.thousandths();
    match write {
        Write::SetVoltage(_) | Write::PresetVoltage(..) | Write::Ovp(_) | Write::Lvp(_) => {
            format!("{} V", limits::two_decimals(milli))
        }
        Write::SetCurrent(_) | Write::PresetCurrent(..) | Write::Ocp(_) => {
            format!("{}.{:03} A", milli / 1000, milli % 1000)
        }
        Write::Opp(_) => format!("{} W", limits::one_decimal(milli)),
        Write::Otp(_) => format!("{} °C", limits::one_decimal(milli)),
        Write::Output(on) => format!("output {}", if on { "ON" } else { "OFF" }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use device::Cause;
    use protocol::value::{MilliCelsius, Milliwatts};
    use protocol::{Dump, Mode, OutputReading, Preset};

    fn preset(volts: u32, amps: u32) -> Preset {
        Preset {
            voltage: Millivolts::new(volts),
            current: Milliamps::new(amps),
        }
    }

    /// Output OFF, 5.00 V / 1.500 A, live maximum 20.05 V / 5.10 A.
    fn dump() -> Dump {
        Dump {
            input_voltage: 20.25,
            set_voltage: Millivolts::new(5000),
            set_current: Milliamps::new(1500),
            output: OutputReading {
                voltage: 0.0,
                current: 0.0,
                power: 0.0,
            },
            temperature: 24.3,
            presets: [preset(5000, 1000); 6],
            ovp: Millivolts::new(30_000),
            ocp: Milliamps::new(5_100),
            opp: Milliwatts::new(150_000),
            otp: MilliCelsius::new(80_000),
            lvp: Millivolts::new(5_000),
            brightness: 3,
            volume: 1,
            flag_d8: 1,
            capacity_ah: 0.0,
            energy_wh: 0.0,
            output_on: false,
            protection: 0,
            mode: Mode::ConstantVoltage,
            flag_e1: 1,
            max_voltage: Millivolts::new(20_050),
            max_current: Milliamps::new(5_100),
            ovp_range: Millivolts::new(30_000),
            ocp_range: Milliamps::new(5_100),
            opp_range: Milliwatts::new(150_000),
            otp_range: MilliCelsius::new(99_000),
            lvp_range: Millivolts::new(30_000),
        }
    }

    fn snapshot(dump: Dump) -> Message {
        Message::Model(Box::new(Snapshot {
            dump,
            model: Some("DPS-150".into()),
            hardware: Some("V1.0".into()),
            firmware: Some("V1.2".into()),
        }))
    }

    fn model() -> Model {
        let mut model = Model::new("/dev/x", true);
        model.apply(snapshot(dump()));
        model
    }

    fn write_of(effect: Effect) -> Write {
        match effect {
            Effect::Write(write) => write,
            other => panic!("expected a write, got {other:?}"),
        }
    }

    fn type_text(model: &mut Model, text: &str) {
        for c in text.chars() {
            model.on_key(Key::Char(c));
        }
    }

    fn volts(mv: u32) -> Write {
        Write::SetVoltage(Millivolts::new(mv))
    }

    // ---- focus and cursor ---------------------------------------------------

    #[test]
    fn edt_01_voltage_has_focus_first_with_the_documented_cursors() {
        let model = Model::new("/dev/x", true);
        assert_eq!(model.focus, Focus::Voltage);
        assert_eq!(model.cursor(Focus::Voltage), 1);
        assert_eq!(model.cursor(Focus::Current), 1);
    }

    #[test]
    fn edt_01_tab_toggles_focus_and_each_quantity_remembers_its_cursor() {
        let mut model = Model::new("/dev/x", true);
        model.cursor_right();
        model.toggle_focus();
        model.cursor_left();
        model.toggle_focus();
        assert_eq!(model.cursor(Focus::Voltage), 2);
        assert_eq!(model.cursor(Focus::Current), 0);
    }

    #[test]
    fn edt_02_the_cursor_stops_at_both_ends() {
        let mut model = Model::new("/dev/x", true);
        for _ in 0..10 {
            model.cursor_left();
        }
        assert_eq!(model.cursor(Focus::Voltage), 0);
        for _ in 0..10 {
            model.cursor_right();
        }
        assert_eq!(model.cursor(Focus::Voltage), 3);
    }

    // ---- knob steps -----------------------------------------------------------

    #[test]
    fn edt_03_up_adds_the_digit_under_the_cursor_and_sends_it() {
        let mut model = model();
        // Cursor on the units digit: +1 V.
        assert_eq!(write_of(model.on_key(Key::Up)), volts(6000));
        assert_eq!(
            model.set_voltage(),
            Some(Millivolts::new(6000)),
            "shown at once"
        );
        model.cursor_right(); // tenths
        assert_eq!(write_of(model.on_key(Key::Down)), volts(5900));
        model.cursor_left();
        model.cursor_left(); // tens
        assert_eq!(write_of(model.on_key(Key::Up)), volts(15_900));
    }

    #[test]
    fn edt_03_the_current_uses_its_own_weights_and_cursor() {
        let mut model = model();
        model.toggle_focus();
        // Tenths: +0.1 A.
        assert_eq!(
            write_of(model.on_key(Key::Up)),
            Write::SetCurrent(Milliamps::new(1600))
        );
        model.cursor_right();
        model.cursor_right(); // thousandths
        assert_eq!(
            write_of(model.on_key(Key::Down)),
            Write::SetCurrent(Milliamps::new(1599))
        );
    }

    #[test]
    fn lim_08_a_step_stops_at_the_live_maximum_with_a_notice() {
        let mut d = dump();
        d.set_voltage = Millivolts::new(20_000);
        let mut model = Model::new("/dev/x", true);
        model.apply(snapshot(d));
        assert_eq!(
            model.on_key(Key::Up),
            Effect::Write(volts(20_050)),
            "stops just there"
        );
        assert_eq!(
            model.message(false).unwrap().text,
            "At live maximum 20.05 V"
        );
        // A further step changes nothing and says so again.
        assert_eq!(model.on_key(Key::Up), Effect::None);
        assert_eq!(
            model.message(false).unwrap().text,
            "At live maximum 20.05 V"
        );
    }

    #[test]
    fn lim_08_a_step_down_to_zero_then_at_minimum() {
        let mut d = dump();
        d.set_voltage = Millivolts::new(500);
        let mut model = Model::new("/dev/x", true);
        model.apply(snapshot(d));
        assert_eq!(write_of(model.on_key(Key::Down)), volts(0));
        assert_eq!(model.on_key(Key::Down), Effect::None);
        assert_eq!(model.message(false).unwrap().text, "At minimum 0.00 V");
    }

    #[test]
    fn lim_05_a_step_stops_just_below_ovp_and_names_it() {
        let mut d = dump();
        d.ovp = Millivolts::new(7_000);
        let mut model = Model::new("/dev/x", true);
        model.apply(snapshot(d));
        assert_eq!(write_of(model.on_key(Key::Up)), volts(6000));
        assert_eq!(write_of(model.on_key(Key::Up)), volts(6990));
        assert_eq!(model.message(false).unwrap().text, "Limited by OVP 7.00 V");
    }

    // ---- typed entry -----------------------------------------------------------

    #[test]
    fn edt_05_entry_starts_with_enter_v_i_or_a_digit() {
        let mut model = model();
        model.on_key(Key::Enter);
        assert_eq!(model.entry().unwrap().focus, Focus::Voltage);
        assert_eq!(model.entry().unwrap().text, "");
        model.on_key(Key::Esc);
        assert!(model.entry().is_none());

        model.on_key(Key::Char('i'));
        assert_eq!(model.focus, Focus::Current, "i moves the focus");
        assert_eq!(model.entry().unwrap().focus, Focus::Current);
        model.on_key(Key::Esc);

        model.on_key(Key::Char('7'));
        assert_eq!(
            model.entry().unwrap().text,
            "7",
            "a digit starts it with itself"
        );
        model.on_key(Key::Esc);
        model.on_key(Key::Char('.'));
        assert_eq!(model.entry().unwrap().text, ".");
    }

    #[test]
    fn edt_05_a_bare_digit_never_recalls_a_preset() {
        let mut model = model();
        assert_eq!(model.on_key(Key::Char('3')), Effect::None);
        assert_eq!(model.entry().unwrap().text, "3");
    }

    #[test]
    fn edt_06_entry_takes_digits_one_dot_and_five_characters() {
        let mut model = model();
        model.on_key(Key::Enter);
        type_text(&mut model, "1.2.3456");
        assert_eq!(
            model.entry().unwrap().text,
            "1.234",
            "second dot ignored, five max"
        );
        model.on_key(Key::Backspace);
        assert_eq!(model.entry().unwrap().text, "1.23");
        type_text(&mut model, "xvi-");
        assert_eq!(
            model.entry().unwrap().text,
            "1.23",
            "other keys are ignored"
        );
        assert_eq!(
            model.entry().unwrap().focus,
            Focus::Voltage,
            "v and i do nothing in entry"
        );
        model.on_key(Key::Backspace);
        model.on_key(Key::Backspace);
        model.on_key(Key::Backspace);
        model.on_key(Key::Backspace);
        model.on_key(Key::Backspace);
        assert_eq!(
            model.entry().unwrap().text,
            "",
            "backspace on empty is fine"
        );
    }

    #[test]
    fn edt_06_space_and_ctrl_x_cancel_the_entry_and_esc_too() {
        for key in [Key::Char(' '), Key::CtrlX, Key::Esc] {
            let mut model = model();
            model.on_key(Key::Enter);
            type_text(&mut model, "9");
            assert_eq!(model.on_key(key), Effect::None, "{key:?}");
            assert!(model.entry().is_none(), "{key:?}");
        }
    }

    #[test]
    fn lim_09_a_typed_value_is_rounded_then_checked_then_sent() {
        let mut model = model();
        model.on_key(Key::Enter);
        type_text(&mut model, "5.255");
        assert_eq!(write_of(model.on_key(Key::Enter)), volts(5_260));
        assert!(model.entry().is_none());
        assert_eq!(model.set_voltage(), Some(Millivolts::new(5_260)));
    }

    #[test]
    fn lim_08_an_out_of_range_value_is_rejected_not_clamped() {
        let mut model = model();
        type_text(&mut model, "35");
        assert_eq!(model.on_key(Key::Enter), Effect::None);
        let entry = model.entry().unwrap();
        assert!(entry.rejected, "the entry stays open, text turns er");
        assert_eq!(entry.text, "35");
        let line = model.message(false).unwrap();
        assert_eq!(line.text, "35 V is above the live maximum 20.05 V");
        assert_eq!(line.tone, Tone::Error);
        assert_eq!(
            model.set_voltage(),
            Some(Millivolts::new(5000)),
            "setpoint unchanged"
        );
        assert_eq!(model.message(true).unwrap().text, "35 V > 20.05 V live max");
        // The next key press clears the message and the mark.
        model.on_key(Key::Backspace);
        assert!(!model.entry().unwrap().rejected);
        assert_eq!(
            model.message(false).unwrap().text,
            "Typing a voltage. Range 0 – 20.05 V (live maximum)"
        );
    }

    #[test]
    fn lim_05_a_typed_value_at_or_above_ovp_is_rejected() {
        let mut d = dump();
        d.ovp = Millivolts::new(13_000);
        let mut model = Model::new("/dev/x", true);
        model.apply(snapshot(d));
        type_text(&mut model, "13");
        assert_eq!(model.on_key(Key::Enter), Effect::None);
        assert_eq!(
            model.message(false).unwrap().text,
            "13 V is not below OVP 13.00 V. Change OVP on the Protection screen (o)"
        );
        model.on_key(Key::Backspace);
        model.on_key(Key::Backspace);
        type_text(&mut model, "12.99");
        assert_eq!(write_of(model.on_key(Key::Enter)), volts(12_990));
    }

    #[test]
    fn lim_09_an_empty_or_unparsable_entry_is_never_sent() {
        let mut model = model();
        model.on_key(Key::Enter);
        assert_eq!(model.on_key(Key::Enter), Effect::None);
        assert!(model.entry().is_none(), "empty cancels");
        model.on_key(Key::Char('.'));
        assert_eq!(model.on_key(Key::Enter), Effect::None);
        assert!(model.entry().unwrap().rejected);
        assert_eq!(model.message(false).unwrap().text, ". is not a number");
    }

    #[test]
    fn edt_06_the_entry_for_the_current_checks_the_current_rules() {
        let mut model = model();
        model.on_key(Key::Char('i'));
        type_text(&mut model, "6");
        assert_eq!(model.on_key(Key::Enter), Effect::None);
        assert_eq!(
            model.message(false).unwrap().text,
            "6 A is above the live maximum 5.10 A"
        );
        model.on_key(Key::Backspace);
        type_text(&mut model, "2.5");
        assert_eq!(
            write_of(model.on_key(Key::Enter)),
            Write::SetCurrent(Milliamps::new(2500))
        );
    }

    // ---- guards ----------------------------------------------------------------

    #[test]
    fn lnk_03_with_the_link_down_nothing_can_be_edited() {
        let mut model = model();
        model.apply(Message::Event(Event::Lost(Cause::MissedPolls)));
        assert_eq!(model.on_key(Key::Up), Effect::None);
        assert_eq!(
            model.message(false).unwrap().text,
            "Link down. Changes are not sent."
        );
        assert_eq!(model.message(true).unwrap().text, "Link down: not sent");
        model.on_key(Key::Enter);
        assert!(model.entry().is_none());
        model.on_key(Key::Char('5'));
        assert!(model.entry().is_none());
    }

    #[test]
    fn lnk_05_typed_entry_and_unconfirmed_values_end_with_the_link() {
        let mut model = model();
        model.on_key(Key::Up);
        model.on_key(Key::Enter);
        assert!(model.entry().is_some());
        model.apply(Message::Event(Event::Lost(Cause::MissedPolls)));
        assert!(model.entry().is_none());
        assert_eq!(
            model.set_voltage(),
            Some(Millivolts::new(5000)),
            "the device value again"
        );
    }

    // ---- read-back -----------------------------------------------------------------

    #[test]
    fn edt_04_the_chosen_value_shows_until_the_read_back_confirms_it() {
        let mut model = model();
        model.on_key(Key::Up);
        assert_eq!(model.set_voltage(), Some(Millivolts::new(6000)));
        // A dump that predates the write must not flip the display back.
        model.apply(snapshot(dump()));
        assert_eq!(model.set_voltage(), Some(Millivolts::new(6000)));
        let mut after = dump();
        after.set_voltage = Millivolts::new(6000);
        model.apply(snapshot(after));
        model.apply(Message::Event(Event::ReadBack {
            written: volts(6000),
            device: volts(6000),
        }));
        assert_eq!(model.set_voltage(), Some(Millivolts::new(6000)));
        assert!(model.message(false).is_none(), "no notice when it matches");
    }

    #[test]
    fn prt_03_a_value_the_device_kept_replaces_the_display_with_a_notice() {
        let mut model = model();
        model.on_key(Key::Up);
        let mut after = dump();
        after.set_voltage = Millivolts::new(4000);
        model.apply(snapshot(after));
        model.apply(Message::Event(Event::ReadBack {
            written: volts(6000),
            device: volts(4000),
        }));
        assert_eq!(model.set_voltage(), Some(Millivolts::new(4000)));
        assert_eq!(model.message(false).unwrap().text, "Device kept 4.00 V");
    }

    #[test]
    fn prt_03_a_newer_pending_value_survives_the_read_back_of_an_older_one() {
        let mut model = model();
        model.on_key(Key::Up); // 6.00
        model.on_key(Key::Up); // 7.00
        model.apply(Message::Event(Event::ReadBack {
            written: volts(6000),
            device: volts(6000),
        }));
        assert_eq!(model.set_voltage(), Some(Millivolts::new(7000)));
    }

    #[test]
    fn app_12_a_refused_write_is_shown_and_not_kept() {
        let mut model = model();
        model.on_key(Key::Up);
        model.apply(Message::Event(Event::WriteRefused {
            write: volts(6000),
            why: Refusal::OutOfRange,
        }));
        assert_eq!(model.set_voltage(), Some(Millivolts::new(5000)));
        let line = model.message(false).unwrap();
        assert_eq!(line.tone, Tone::Error);
        assert!(line.text.contains("outside the device range"));
    }

    // ---- notices ------------------------------------------------------------------

    #[test]
    fn a_notice_goes_after_three_seconds_or_on_the_next_key() {
        let mut model = model();
        model.notify("hello");
        assert_eq!(model.message(false).unwrap().text, "hello");
        model.tick(Duration::from_millis(2900));
        assert!(model.message(false).is_some());
        model.tick(Duration::from_secs(3));
        assert!(model.message(false).is_none());
        model.notify("again");
        model.on_key(Key::Tab);
        assert!(model.message(false).is_none());
    }

    #[test]
    fn lim_07_a_setpoint_above_the_live_maximum_gets_the_persistent_warning() {
        let mut d = dump();
        d.set_voltage = Millivolts::new(20_000);
        d.max_voltage = Millivolts::new(18_400);
        let mut model = Model::new("/dev/x", true);
        model.apply(snapshot(d));
        assert_eq!(
            model.message(false).unwrap().text,
            "Set 20.00 V is above the live maximum 18.40 V. Lower it to turn output ON"
        );
        assert_eq!(
            model.message(true).unwrap().text,
            "V 20.00 above live max 18.40"
        );
        // It does not go away with a key press, and the setpoint is untouched.
        model.on_key(Key::Tab);
        assert!(model.message(false).is_some());
        assert_eq!(model.set_voltage(), Some(Millivolts::new(20_000)));
        // Editing stays possible.
        model.toggle_focus();
        model.toggle_focus();
        assert!(matches!(model.on_key(Key::Down), Effect::Write(_)));
    }

    #[test]
    fn lim_07_the_current_setpoint_has_the_same_warning() {
        let mut d = dump();
        d.set_current = Milliamps::new(3_000);
        d.max_current = Milliamps::new(2_500);
        let mut model = Model::new("/dev/x", true);
        model.apply(snapshot(d));
        assert_eq!(
            model.message(false).unwrap().text,
            "Set 3.000 A is above the live maximum 2.50 A. Lower it to turn output ON"
        );
    }

    #[test]
    fn ui_09_the_warning_and_hints_fit_their_rows() {
        let mut d = dump();
        d.set_voltage = Millivolts::new(30_000);
        d.max_voltage = Millivolts::new(18_400);
        let mut model = Model::new("/dev/x", true);
        model.apply(snapshot(d));
        let line = model.message(false).unwrap().text;
        assert!(line.chars().count() <= 76, "{line}");
        assert!(model.message(true).unwrap().text.chars().count() <= 43);
    }

    // ---- keys that are not edits ----------------------------------------------------

    #[test]
    fn q_and_ctrl_c_quit_and_r_retries_only_while_down_with_reconnect_off() {
        let mut model = Model::new("/dev/x", false);
        assert_eq!(model.on_key(Key::Char('r')), Effect::None, "online");
        assert_eq!(model.on_key(Key::Char('q')), Effect::Quit);
        assert_eq!(model.on_key(Key::CtrlC), Effect::Quit);
        model.apply(Message::Event(Event::Lost(Cause::MissedPolls)));
        assert_eq!(model.on_key(Key::Char('r')), Effect::Reconnect);
        model.set_reconnect(true);
        assert_eq!(
            model.on_key(Key::Char('r')),
            Effect::None,
            "automatic already"
        );
    }

    #[test]
    fn ctrl_c_quits_even_in_typed_entry() {
        let mut model = model();
        model.on_key(Key::Enter);
        assert_eq!(model.on_key(Key::CtrlC), Effect::Quit);
    }

    #[test]
    fn lnk_02_loss_and_restore_follow_the_device_events() {
        let mut model = Model::new("/dev/x", false);
        model.apply(Message::Event(Event::Lost(Cause::MissedPolls)));
        assert_eq!(
            model.link,
            LinkStatus::Lost {
                auto_reconnect: false
            }
        );
        model.set_reconnect(true);
        assert_eq!(
            model.link,
            LinkStatus::Lost {
                auto_reconnect: true
            }
        );
        model.apply(Message::Event(Event::Online { restored: true }));
        assert!(model.is_online());
    }
}
