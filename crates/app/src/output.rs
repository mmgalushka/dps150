//! The output switch and what guards it (SAFETY.md §2, §3; DESIGN.md §1.5,
//! §5.12, §9.1): turning ON, the jump guard, the recall prompt, `Space` and
//! `Ctrl-X` in every context, the queued OFF while the link is down, and the
//! tripped state.

use protocol::Write;
use protocol::value::{Milliamps, Millivolts};

use crate::constants::NOTICE_DURATION;
use crate::limits::{Bounds, Reject, Text, one_decimal, two_decimals};
use crate::model::{Effect, Model, Tone};
use crate::protection::ALL;
use crate::screens::Prompt;

/// Settings → Safety (APP.md §4.1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Safety {
    /// "Output on = ask first" (the default) or "immediately".
    pub ask_on: bool,
    /// Voltage jump threshold in mV; 0 turns the prompt off.
    pub v_jump: u32,
    /// Current jump threshold in mA; 0 turns the prompt off.
    pub i_jump: u32,
}

impl Default for Safety {
    fn default() -> Self {
        Self {
            ask_on: true,
            v_jump: 5_000,
            i_jump: 2_000,
        }
    }
}

fn amps3(milli: u32) -> String {
    format!("{}.{:03}", milli / 1000, milli % 1000)
}

/// `+10.00 V`, `-7.00 V`.
fn signed_volts(delta: i64) -> String {
    let sign = if delta < 0 { '-' } else { '+' };
    format!("{sign}{} V", two_decimals(delta.unsigned_abs() as u32))
}

fn signed_amps(delta: i64) -> String {
    let sign = if delta < 0 { '-' } else { '+' };
    format!("{sign}{} A", amps3(delta.unsigned_abs() as u32))
}

impl Model {
    pub fn set_safety(&mut self, safety: Safety) {
        self.safety = safety;
    }

    pub fn safety(&self) -> Safety {
        self.safety
    }

    /// Writes the model wants sent that no key caused, such as the queued OFF
    /// after a reconnect. The caller sends them in order.
    pub fn drain_writes(&mut self) -> Vec<Write> {
        std::mem::take(&mut self.outgoing)
    }

    /// An OFF is waiting for the link to come back (LNK-04).
    pub fn off_queued(&self) -> bool {
        self.off_queued
    }

    /// Is a protection tripped? (`DC` is not zero.)
    pub fn tripped(&self) -> Option<crate::protection::Limit> {
        let code = self.snapshot.as_ref()?.dump.protection;
        ALL.get(usize::from(code).checked_sub(1)?).copied()
    }

    /// The terminal is below the 44×16 floor: prompts and typed entry end,
    /// and only OFF and `Ctrl-C` work (SAF-09, SAF-10).
    pub fn set_too_small(&mut self, small: bool) {
        self.too_small = small;
        if small {
            self.cancel_pending();
        }
    }

    pub fn is_too_small(&self) -> bool {
        self.too_small
    }

    /// Drops every prompt, entry and edit: nothing is applied.
    pub(crate) fn cancel_pending(&mut self) {
        self.prompt = None;
        self.chord = false;
        self.entry = None;
        if matches!(self.presets.mode, crate::screens::PresetMode::Edit(_)) {
            self.presets.mode = crate::screens::PresetMode::List;
        }
        self.protection.mode = crate::screens::ProtectionMode::Selecting;
    }

    // ---- OFF, ON, toggle --------------------------------------------------------

    /// OFF only: the plain OFF of `Ctrl-X`, and of `Space` where it is not a
    /// toggle. Never ON. With the link down it queues the OFF (LNK-04).
    pub(crate) fn off_only(&mut self) -> Effect {
        if !self.is_online() {
            self.queue_off(false);
            return Effect::None;
        }
        if self.output_is_on() {
            Effect::Write(Write::Output(false))
        } else {
            Effect::None
        }
    }

    fn queue_off(&mut self, toggle: bool) {
        let text = if toggle && self.off_queued {
            self.off_queued = false;
            "Off queue cleared"
        } else {
            self.off_queued = true;
            "Off queued · use the front-panel button"
        };
        self.same_notice(text, Tone::Warn, Some(self.now + NOTICE_DURATION));
    }

    /// `Space` as a plain toggle.
    pub(crate) fn toggle_output(&mut self) -> Effect {
        if !self.is_online() {
            self.queue_off(true);
            return Effect::None;
        }
        if self.snapshot.is_none() {
            return Effect::None;
        }
        if self.output_is_on() {
            // OFF never asks (SAF-04).
            return Effect::Write(Write::Output(false));
        }
        self.request_on()
    }

    /// The setpoints as shown, in mV and mA.
    fn shown(&self) -> Option<(u32, u32)> {
        Some((self.set_voltage()?.get(), self.set_current()?.get()))
    }

    /// May the output go ON at the setpoints shown? The live-maximum rule
    /// (LIM-07) and the ordering rules (LIM-04, LIM-06).
    fn why_not_on(&self) -> Option<Text> {
        let dump = &self.snapshot.as_ref()?.dump;
        let (voltage, current) = self.shown()?;
        let bounds = Bounds::new(dump, voltage, current);
        let blocked = |standard: &str, compact: &str| {
            Some(Text {
                standard: standard.to_string(),
                compact: compact.to_string(),
            })
        };
        match bounds.check_voltage(voltage) {
            Err(Reject::AboveLiveMax { .. }) => {
                return blocked(
                    "Cannot turn ON: set voltage is above the live maximum",
                    "ON blocked: V above live max",
                );
            }
            Err(Reject::NotBelowOvp { .. }) => {
                return blocked(
                    "Cannot turn ON: set voltage is not below OVP (o)",
                    "ON blocked: V not below OVP",
                );
            }
            Err(Reject::NotBelowOpp { .. }) => {
                return blocked(
                    "Cannot turn ON: set power is not below OPP (o)",
                    "ON blocked: power not below OPP",
                );
            }
            _ => {}
        }
        match bounds.check_current(current) {
            Err(Reject::AboveLiveMax { .. }) => blocked(
                "Cannot turn ON: set current is above the live maximum",
                "ON blocked: I above live max",
            ),
            Err(Reject::NotBelowOcp { .. }) => blocked(
                "Cannot turn ON: set current is not below OCP (o)",
                "ON blocked: I not below OCP",
            ),
            Err(Reject::NotBelowOpp { .. }) => blocked(
                "Cannot turn ON: set power is not below OPP (o)",
                "ON blocked: power not below OPP",
            ),
            _ => None,
        }
    }

    /// Turning the output ON, or clearing a trip, which is the same thing
    /// (SAF-13).
    fn request_on(&mut self) -> Effect {
        if let Some(text) = self.why_not_on() {
            self.set_notice(text, Tone::Warn, Some(self.now + NOTICE_DURATION));
            return Effect::None;
        }
        let Some((voltage, current)) = self.shown() else {
            return Effect::None;
        };
        if let Some(trip) = self.tripped() {
            // A trip is always asked about, even with "immediately".
            let cause = self.cause_still_present(trip);
            self.prompt = Some(Prompt::ClearTrip {
                trip: trip.name().to_string(),
                voltage,
                current,
                cause,
            });
            return Effect::None;
        }
        if self.safety.ask_on {
            self.prompt = Some(Prompt::OutputOn { voltage, current });
            return Effect::None;
        }
        self.turn_on(voltage, current)
    }

    /// For the causes the host can see with the output off: temperature and
    /// input voltage. The others depend on the load (SAFETY.md §3.3).
    fn cause_still_present(&self, trip: crate::protection::Limit) -> Option<String> {
        use crate::protection::Limit;
        let dump = &self.snapshot.as_ref()?.dump;
        let milli = |x: f32| (f64::from(x.max(0.0)) * 1000.0).round() as u32;
        match trip {
            Limit::Otp if milli(dump.temperature) > dump.otp.get() => Some(format!(
                "{} °C > {} °C",
                one_decimal(milli(dump.temperature)),
                one_decimal(dump.otp.get())
            )),
            Limit::Lvp if milli(dump.input_voltage) < dump.lvp.get() => Some(format!(
                "{} V < {} V",
                two_decimals(milli(dump.input_voltage)),
                two_decimals(dump.lvp.get())
            )),
            _ => None,
        }
    }

    fn turn_on(&mut self, voltage: u32, current: u32) -> Effect {
        // The reference of the jump guard is the setpoint at this ON (§2).
        self.reference = Some((voltage, current));
        Effect::Write(Write::Output(true))
    }

    // ---- the jump guard ------------------------------------------------------------

    /// The reference follows the setpoint while the output is OFF, and is
    /// taken the first time an ON is seen (§2).
    pub(crate) fn follow_reference(&mut self) {
        let Some(on) = self.snapshot.as_ref().map(|s| s.dump.output_on) else {
            return;
        };
        if !on || self.reference.is_none() {
            // The setpoint as shown, so a value still waiting for its
            // read-back counts.
            self.reference = self.shown();
        }
    }

    /// Sends `writes` (setpoint changes that end at `voltage` / `current`),
    /// or holds them and asks, when the output is ON and the change takes a
    /// setpoint beyond its threshold from the reference (SAF-06).
    pub(crate) fn guarded(&mut self, writes: Vec<Write>, voltage: u32, current: u32) -> Effect {
        if self.output_is_on() {
            let (ref_v, ref_i) = self.reference.unwrap_or((voltage, current));
            let dv = i64::from(voltage) - i64::from(ref_v);
            let di = i64::from(current) - i64::from(ref_i);
            let crossed_v =
                self.safety.v_jump > 0 && dv.unsigned_abs() > u64::from(self.safety.v_jump);
            let crossed_i =
                self.safety.i_jump > 0 && di.unsigned_abs() > u64::from(self.safety.i_jump);
            if crossed_v || crossed_i {
                self.prompt = Some(Prompt::Jump {
                    voltage,
                    current,
                    dv: crossed_v.then_some(dv),
                    di: crossed_i.then_some(di),
                    writes,
                });
                return Effect::None;
            }
        }
        self.send_all(writes)
    }

    pub(crate) fn send_all(&mut self, writes: Vec<Write>) -> Effect {
        for write in &writes {
            self.stage_write(*write);
        }
        if !self.output_is_on() {
            self.reference = self.shown();
        }
        match writes.len() {
            0 => Effect::None,
            1 => Effect::Write(writes[0]),
            _ => Effect::Writes(writes),
        }
    }

    /// A recall with the output ON always asks (SAF-08).
    pub(crate) fn recall_while_on(&mut self, slot: usize, voltage: u32, current: u32) {
        self.prompt = Some(Prompt::Recall {
            slot,
            voltage,
            current,
        });
    }

    // ---- prompt answers --------------------------------------------------------------

    /// `y` on a prompt of this module. `None` when it is not one of its own.
    pub(crate) fn confirm_output_prompt(&mut self, prompt: &Prompt) -> Option<Effect> {
        Some(match prompt {
            Prompt::OutputOn { voltage, current }
            | Prompt::ClearTrip {
                voltage, current, ..
            } => {
                // The values may have moved while the question was open; the
                // question named these, so they are what is applied.
                let writes = vec![
                    Write::SetVoltage(Millivolts::new(*voltage)),
                    Write::SetCurrent(Milliamps::new(*current)),
                ];
                let shown = self.shown();
                let mut sends = Vec::new();
                if shown != Some((*voltage, *current)) {
                    sends.extend(writes);
                }
                sends.push(Write::Output(true));
                self.reference = Some((*voltage, *current));
                for write in &sends {
                    if !matches!(write, Write::Output(_)) {
                        self.stage_write(*write);
                    }
                }
                match sends.len() {
                    1 => Effect::Write(sends[0]),
                    _ => Effect::Writes(sends),
                }
            }
            Prompt::Jump {
                voltage,
                current,
                writes,
                ..
            } => {
                self.reference = Some((*voltage, *current));
                self.send_all(writes.clone())
            }
            Prompt::Recall {
                slot,
                voltage,
                current,
            } => {
                self.reference = Some((*voltage, *current));
                let _ = slot;
                self.send_all(vec![
                    Write::SetVoltage(Millivolts::new(*voltage)),
                    Write::SetCurrent(Milliamps::new(*current)),
                ])
            }
            Prompt::Overwrite { .. } | Prompt::Clear { .. } => return None,
        })
    }

    /// The question text of this module's prompts (DESIGN.md §10).
    pub(crate) fn output_prompt_text(&self, prompt: &Prompt, compact: bool) -> Option<String> {
        Some(match prompt {
            Prompt::OutputOn { voltage, current } => {
                let (v, a) = (two_decimals(*voltage), amps3(*current));
                if compact {
                    format!("ON at {v} V / {a} A?")
                } else {
                    format!("Turn output ON at {v} V / {a} A?")
                }
            }
            Prompt::ClearTrip {
                trip,
                voltage,
                current,
                cause,
            } => {
                let (v, a) = (two_decimals(*voltage), amps3(*current));
                match (compact, cause) {
                    (true, _) => format!("Clear {trip}, ON at {v} V / {a} A?"),
                    (false, None) => format!("Clear {trip} and turn output ON at {v} V / {a} A?"),
                    (false, Some(cause)) => {
                        format!("Clear {trip} ({cause}) and turn ON at {v} V / {a} A?")
                    }
                }
            }
            Prompt::Jump {
                voltage,
                current,
                dv,
                di,
                ..
            } => {
                let (v, a) = (two_decimals(*voltage), amps3(*current));
                if compact {
                    match (dv, di) {
                        (Some(dv), None) => format!("Set {v} V ({})?", signed_volts(*dv)),
                        (None, Some(di)) => format!("Set {a} A ({})?", signed_amps(*di)),
                        _ => format!("Set {v} V, {a} A?"),
                    }
                } else {
                    let parts: Vec<String> = [dv.map(signed_volts), di.map(signed_amps)]
                        .into_iter()
                        .flatten()
                        .collect();
                    format!("Set {v} V / {a} A ({}), output ON?", parts.join(", "))
                }
            }
            Prompt::Recall {
                slot,
                voltage,
                current,
            } => {
                let (v, a) = (two_decimals(*voltage), amps3(*current));
                if compact {
                    format!("Recall M{} {v}V/{a}A?", slot + 1)
                } else {
                    format!("Recall M{} {v} V / {a} A, output ON?", slot + 1)
                }
            }
            Prompt::Overwrite { .. } | Prompt::Clear { .. } => return None,
        })
    }
}
