//! The model the main panel draws (APP.md §2, §7): one snapshot owned by the
//! app thread, updated from device messages and key presses.

use device::{Event, Message, Snapshot};

/// Which readout has the focus (EDT-01).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Voltage,
    Current,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    None,
    Quit,
    /// One reconnect attempt (`r`, Reconnect = off).
    Reconnect,
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
    /// A notice for the message line, cleared by the next key press (APP-05).
    pub notice: Option<String>,
    reconnect: bool,
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
            notice: None,
            reconnect,
        }
    }

    pub fn set_reconnect(&mut self, on: bool) {
        self.reconnect = on;
        if let LinkStatus::Lost { .. } = self.link {
            self.link = LinkStatus::Lost { auto_reconnect: on };
        }
    }

    pub fn apply(&mut self, message: Message) {
        match message {
            Message::Model(snapshot) => self.snapshot = Some(*snapshot),
            Message::Event(Event::Online { .. }) => self.link = LinkStatus::Online,
            Message::Event(Event::Lost(_) | Event::Failed(_)) => {
                self.link = LinkStatus::Lost {
                    auto_reconnect: self.reconnect,
                };
            }
        }
    }

    /// Digit index of `focus`: 0..=3, counting digits only (skips the `.`).
    pub fn cursor(&self, focus: Focus) -> usize {
        match focus {
            Focus::Voltage => self.voltage_cursor,
            Focus::Current => self.current_cursor,
        }
    }

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

    /// Read-only stage: keys move focus and the cursor locally; nothing is
    /// written to the device. `Space`, `Ctrl-X`, digits and the arrow-up/down
    /// edits arrive with the write path (PLAN.md §3, M5 and M6).
    pub fn on_key(&mut self, key: Key) -> Effect {
        self.notice = None;
        match key {
            Key::Char('q') | Key::CtrlC => Effect::Quit,
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

    pub fn is_online(&self) -> bool {
        self.link == LinkStatus::Online
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn keys_in_the_read_only_stage_move_locally_and_never_write() {
        let mut model = Model::new("/dev/x", true);
        model.notice = Some("note".into());
        assert_eq!(model.on_key(Key::Tab), Effect::None);
        assert_eq!(model.focus, Focus::Current);
        assert_eq!(model.notice, None, "any key clears the notice");
        assert_eq!(model.on_key(Key::Right), Effect::None);
        assert_eq!(model.cursor(Focus::Current), 2);
        for key in [
            Key::Char(' '),
            Key::CtrlX,
            Key::Up,
            Key::Down,
            Key::Enter,
            Key::Char('5'),
        ] {
            assert_eq!(model.on_key(key), Effect::None, "{key:?}");
        }
        assert_eq!(model.cursor(Focus::Current), 2, "nothing else moved");
        assert_eq!(model.on_key(Key::Char('q')), Effect::Quit);
        assert_eq!(model.on_key(Key::CtrlC), Effect::Quit);
    }

    #[test]
    fn lnk_06_r_retries_only_while_down_with_reconnect_off() {
        use device::Cause;
        let mut model = Model::new("/dev/x", false);
        assert_eq!(model.on_key(Key::Char('r')), Effect::None, "online");
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
    fn lnk_02_loss_and_restore_follow_the_device_events() {
        use device::Cause;
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
