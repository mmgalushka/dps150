//! The quit sequence (SAFETY.md §4.9, §4.11; SAF-11, SAF-14): optionally turn
//! the output off and wait for the read-back, close the session, and say what
//! was left behind. Pure, with injected time, like the link core.

use std::time::Duration;

use device::Event;
use protocol::Write;

use crate::constants::{CLOSE_TIMEOUT, QUIT_READBACK};
use crate::limits::two_decimals;

/// What the program knows when the quit starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Facts {
    pub online: bool,
    pub output_on: bool,
    /// The setpoints as shown, for the "left ON" line.
    pub voltage: u32,
    pub current: u32,
    /// Turn the output off first (Settings → On exit, or `o` at the prompt).
    pub off: bool,
}

/// What the caller must do for the sequence.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    Write(Write),
    /// Send the close frame after whatever is queued (SAF-14).
    Close,
}

/// The last words: one line for stdout, printed after the terminal is
/// restored, and the exit status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outcome {
    pub line: Option<String>,
    pub status: u8,
}

#[derive(Clone, Debug)]
enum Stage {
    /// The OFF is out; waiting for the dump that shows it.
    WaitOff {
        deadline: Duration,
    },
    /// The close request is out; waiting for `Closed`.
    Closing {
        deadline: Duration,
    },
    Done,
}

#[derive(Clone, Debug)]
pub struct Shutdown {
    stage: Stage,
    outcome: Outcome,
}

fn left_on(voltage: u32, current: u32) -> String {
    format!(
        "dps150: output left ON ({} V / {}.{:03} A)",
        two_decimals(voltage),
        current / 1000,
        current % 1000
    )
}

impl Shutdown {
    /// Starts the sequence and returns what to do first.
    pub fn start(now: Duration, facts: Facts) -> (Self, Vec<Action>) {
        if !facts.online {
            // Nothing can be sent: say so (SAFETY.md §4.9).
            let done = Self {
                stage: Stage::Done,
                outcome: Outcome {
                    line: Some("dps150: link lost, output state unknown".into()),
                    status: 0,
                },
            };
            return (done, Vec::new());
        }
        if facts.off && facts.output_on {
            let shutdown = Self {
                stage: Stage::WaitOff {
                    deadline: now + QUIT_READBACK,
                },
                outcome: Outcome {
                    line: None,
                    status: 0,
                },
            };
            return (shutdown, vec![Action::Write(Write::Output(false))]);
        }
        let line = facts
            .output_on
            .then(|| left_on(facts.voltage, facts.current));
        let shutdown = Self {
            stage: Stage::Closing {
                deadline: now + CLOSE_TIMEOUT,
            },
            outcome: Outcome { line, status: 0 },
        };
        (shutdown, vec![Action::Close])
    }

    /// An event from the device thread.
    pub fn event(&mut self, now: Duration, event: &Event) -> Vec<Action> {
        match (&self.stage, event) {
            (
                Stage::WaitOff { .. },
                Event::ReadBack {
                    written: Write::Output(false),
                    device,
                },
            ) => {
                if *device == Write::Output(false) {
                    self.outcome.line = Some("dps150: output turned OFF".into());
                    self.begin_closing(now)
                } else {
                    self.not_confirmed(now)
                }
            }
            (
                Stage::WaitOff { .. },
                Event::WriteRefused { .. } | Event::Lost(_) | Event::Failed(_),
            ) => self.not_confirmed(now),
            (Stage::Closing { .. }, Event::Closed | Event::Lost(_) | Event::Failed(_)) => {
                self.stage = Stage::Done;
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// Time passes: the read-back or the close may have run out of time.
    pub fn tick(&mut self, now: Duration) -> Vec<Action> {
        match self.stage {
            Stage::WaitOff { deadline, .. } if now >= deadline => self.not_confirmed(now),
            Stage::Closing { deadline } if now >= deadline => {
                self.stage = Stage::Done;
                Vec::new()
            }
            _ => Vec::new(),
        }
    }

    /// The user insists (a second `Ctrl-C`): stop waiting. Whatever was not
    /// confirmed stays unconfirmed.
    pub fn abort(&mut self) {
        if matches!(self.stage, Stage::WaitOff { .. }) {
            self.outcome = Outcome {
                line: Some("dps150: could not confirm output OFF, check the unit".into()),
                status: 1,
            };
        }
        self.stage = Stage::Done;
    }

    /// The output did not read back as OFF in time (SAF-11). The session is
    /// closed all the same: that releases the front-panel lock, so the panel
    /// button works.
    fn not_confirmed(&mut self, now: Duration) -> Vec<Action> {
        self.outcome = Outcome {
            line: Some("dps150: could not confirm output OFF, check the unit".into()),
            status: 1,
        };
        self.begin_closing(now)
    }

    fn begin_closing(&mut self, now: Duration) -> Vec<Action> {
        self.stage = Stage::Closing {
            deadline: now + CLOSE_TIMEOUT,
        };
        vec![Action::Close]
    }

    /// The finished outcome, once the sequence is over.
    pub fn finished(&self) -> Option<&Outcome> {
        matches!(self.stage, Stage::Done).then_some(&self.outcome)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const T0: Duration = Duration::from_secs(10);

    fn facts(online: bool, output_on: bool, off: bool) -> Facts {
        Facts {
            online,
            output_on,
            voltage: 12_000,
            current: 1_000,
            off,
        }
    }

    fn ms(n: u64) -> Duration {
        T0 + Duration::from_millis(n)
    }

    fn off() -> Write {
        Write::Output(false)
    }

    #[test]
    fn saf_11_leaving_an_output_that_is_off_closes_at_once_and_prints_nothing() {
        let (mut shutdown, actions) = Shutdown::start(T0, facts(true, false, false));
        assert_eq!(actions, [Action::Close]);
        assert!(shutdown.finished().is_none(), "waits for the close");
        shutdown.event(ms(150), &Event::Closed);
        assert_eq!(
            shutdown.finished(),
            Some(&Outcome {
                line: None,
                status: 0
            })
        );
    }

    #[test]
    fn saf_11_leaving_an_output_that_is_on_says_so_with_the_values() {
        let (mut shutdown, actions) = Shutdown::start(T0, facts(true, true, false));
        assert_eq!(actions, [Action::Close], "no OFF is sent");
        shutdown.event(ms(150), &Event::Closed);
        assert_eq!(
            shutdown.finished().unwrap().line.as_deref(),
            Some("dps150: output left ON (12.00 V / 1.000 A)")
        );
        assert_eq!(shutdown.finished().unwrap().status, 0);
    }

    #[test]
    fn saf_11_turning_off_waits_for_the_read_back_then_closes() {
        let (mut shutdown, actions) = Shutdown::start(T0, facts(true, true, true));
        assert_eq!(actions, [Action::Write(off())], "the OFF first");
        assert!(shutdown.tick(ms(100)).is_empty(), "nothing else yet");
        let actions = shutdown.event(
            ms(220),
            &Event::ReadBack {
                written: off(),
                device: off(),
            },
        );
        assert_eq!(actions, [Action::Close], "the close follows the read-back");
        assert!(shutdown.finished().is_none());
        shutdown.event(ms(400), &Event::Closed);
        assert_eq!(
            shutdown.finished(),
            Some(&Outcome {
                line: Some("dps150: output turned OFF".into()),
                status: 0
            })
        );
    }

    #[test]
    fn saf_11_an_off_that_reads_back_as_on_is_not_confirmed_and_exits_1() {
        let (mut shutdown, _) = Shutdown::start(T0, facts(true, true, true));
        let actions = shutdown.event(
            ms(220),
            &Event::ReadBack {
                written: off(),
                device: Write::Output(true),
            },
        );
        assert_eq!(
            actions,
            [Action::Close],
            "the session is closed all the same"
        );
        shutdown.event(ms(400), &Event::Closed);
        let outcome = shutdown.finished().unwrap();
        assert_eq!(
            outcome.line.as_deref(),
            Some("dps150: could not confirm output OFF, check the unit")
        );
        assert_eq!(outcome.status, 1);
    }

    #[test]
    fn saf_11_no_read_back_within_500_ms_is_not_confirmed() {
        let (mut shutdown, _) = Shutdown::start(T0, facts(true, true, true));
        assert!(shutdown.tick(ms(499)).is_empty());
        assert_eq!(shutdown.tick(ms(500)), [Action::Close]);
        shutdown.event(ms(700), &Event::Closed);
        assert_eq!(shutdown.finished().unwrap().status, 1);
    }

    #[test]
    fn saf_11_a_lost_link_or_a_refused_write_while_waiting_is_not_confirmed() {
        for event in [
            Event::Lost(device::Cause::MissedPolls),
            Event::WriteRefused {
                write: off(),
                why: device::Refusal::NotOnline,
            },
        ] {
            let (mut shutdown, _) = Shutdown::start(T0, facts(true, true, true));
            assert_eq!(shutdown.event(ms(100), &event), [Action::Close]);
            shutdown.tick(ms(1_700));
            assert_eq!(shutdown.finished().unwrap().status, 1, "{event:?}");
        }
    }

    #[test]
    fn saf_11_with_the_link_down_nothing_is_sent_and_the_line_says_so() {
        let (shutdown, actions) = Shutdown::start(T0, facts(false, true, true));
        assert!(actions.is_empty());
        let outcome = shutdown.finished().unwrap();
        assert_eq!(
            outcome.line.as_deref(),
            Some("dps150: link lost, output state unknown")
        );
        assert_eq!(outcome.status, 0);
    }

    #[test]
    fn saf_11_turning_off_an_output_that_is_already_off_sends_no_write() {
        let (_, actions) = Shutdown::start(T0, facts(true, false, true));
        assert_eq!(actions, [Action::Close]);
    }

    #[test]
    fn saf_14_a_close_that_never_confirms_ends_after_the_timeout() {
        let (mut shutdown, _) = Shutdown::start(T0, facts(true, false, false));
        assert!(shutdown.tick(ms(1_499)).is_empty());
        assert!(shutdown.finished().is_none());
        shutdown.tick(ms(1_500));
        assert!(shutdown.finished().is_some());
    }

    #[test]
    fn saf_14_a_link_loss_while_closing_ends_the_sequence() {
        let (mut shutdown, _) = Shutdown::start(T0, facts(true, false, false));
        shutdown.event(ms(50), &Event::Lost(device::Cause::MissedPolls));
        assert!(shutdown.finished().is_some());
    }

    #[test]
    fn saf_11_a_second_ctrl_c_stops_waiting_and_an_unconfirmed_off_stays_unconfirmed() {
        let (mut shutdown, _) = Shutdown::start(T0, facts(true, true, true));
        shutdown.abort();
        let outcome = shutdown.finished().unwrap();
        assert_eq!(outcome.status, 1);
        assert!(
            outcome
                .line
                .as_deref()
                .unwrap()
                .contains("could not confirm")
        );
        // After the OFF was confirmed an abort changes nothing.
        let (mut shutdown, _) = Shutdown::start(T0, facts(true, true, true));
        shutdown.event(
            ms(100),
            &Event::ReadBack {
                written: off(),
                device: off(),
            },
        );
        shutdown.abort();
        assert_eq!(shutdown.finished().unwrap().status, 0);
        assert_eq!(
            shutdown.finished().unwrap().line.as_deref(),
            Some("dps150: output turned OFF")
        );
    }

    #[test]
    fn saf_11_unrelated_events_change_nothing() {
        let (mut shutdown, _) = Shutdown::start(T0, facts(true, true, true));
        let other = Event::ReadBack {
            written: Write::SetVoltage(protocol::value::Millivolts::new(1)),
            device: Write::SetVoltage(protocol::value::Millivolts::new(1)),
        };
        assert!(shutdown.event(ms(50), &other).is_empty());
        assert!(shutdown.event(ms(50), &Event::Closed).is_empty());
        assert!(shutdown.finished().is_none());
    }
}
