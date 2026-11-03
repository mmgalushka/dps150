//! The fault-injection commands of SIMULATOR.md §5.

use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use protocol::Register;

use crate::device::{Load, Protection, SimDevice};

/// One line of the control language (SIM-11).
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Load(Load),
    Vin(f32),
    Temp(f32),
    Trip(Protection),
    Keep(u8, f64),
    Unplug,
    Plug,
    Close,
    Reopen,
    Corrupt(u32),
    Split(u32),
    Delay(Duration),
    PowerCycle,
    State,
    Quit,
}

/// Why a line is not a command; its text is what `error: <reason>` prints.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError(String);

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParseError {}

fn fail<T>(reason: impl Into<String>) -> Result<T, ParseError> {
    Err(ParseError(reason.into()))
}

fn number(word: Option<&str>, what: &str) -> Result<f64, ParseError> {
    let Some(word) = word else {
        return fail(format!("missing {what}"));
    };
    match word.parse::<f64>() {
        Ok(value) if value.is_finite() => Ok(value),
        _ => fail(format!("{what} is not a number: {word}")),
    }
}

fn count(word: Option<&str>, what: &str) -> Result<u32, ParseError> {
    let Some(word) = word else {
        return fail(format!("missing {what}"));
    };
    word.parse()
        .or_else(|_| fail(format!("{what} is not a whole number: {word}")))
}

fn type_byte(word: Option<&str>) -> Result<u8, ParseError> {
    let Some(word) = word else {
        return fail("missing type");
    };
    let digits = word.trim_start_matches("0x").trim_start_matches("0X");
    let id = u8::from_str_radix(digits, 16)
        .or_else(|_| fail(format!("type is not a hex byte: {word}")))?;
    if is_writable(id) {
        Ok(id)
    } else {
        fail(format!("type {id:02X} cannot be written"))
    }
}

/// The types marked W in PROTOCOL.md §4.
fn is_writable(id: u8) -> bool {
    matches!(
        Register::from_id(id),
        Some(
            Register::SetVoltage
                | Register::SetCurrent
                | Register::PresetVoltage(_)
                | Register::PresetCurrent(_)
                | Register::Ovp
                | Register::Ocp
                | Register::Opp
                | Register::Otp
                | Register::Lvp
                | Register::Brightness
                | Register::Volume
                | Register::OutputState
        )
    )
}

impl FromStr for Command {
    type Err = ParseError;

    fn from_str(line: &str) -> Result<Self, ParseError> {
        let mut words = line.split_whitespace();
        let Some(name) = words.next() else {
            return fail("empty line");
        };
        let command = match name.to_ascii_lowercase().as_str() {
            "load" => match words.next() {
                Some("open") => Self::Load(Load::Open),
                word => {
                    let ohms = number(word, "ohms")?;
                    if ohms <= 0.0 {
                        return fail("ohms must be positive");
                    }
                    Self::Load(Load::Ohms(ohms))
                }
            },
            "vin" => {
                let volts = number(words.next(), "volts")?;
                if volts < 0.0 {
                    return fail("volts must not be negative");
                }
                Self::Vin(volts as f32)
            }
            "temp" => Self::Temp(number(words.next(), "celsius")? as f32),
            "trip" => Self::Trip(match words.next() {
                Some("ovp") => Protection::Ovp,
                Some("ocp") => Protection::Ocp,
                Some("opp") => Protection::Opp,
                Some("otp") => Protection::Otp,
                Some("lvp") => Protection::Lvp,
                Some(other) => return fail(format!("unknown protection: {other}")),
                None => return fail("missing protection"),
            }),
            "keep" => {
                let id = type_byte(words.next())?;
                Self::Keep(id, number(words.next(), "value")?)
            }
            "unplug" => Self::Unplug,
            "plug" => Self::Plug,
            "close" => Self::Close,
            "reopen" => Self::Reopen,
            "corrupt" => Self::Corrupt(count(words.next(), "count")?),
            "split" => Self::Split(count(words.next(), "count")?),
            "delay" => Self::Delay(Duration::from_millis(u64::from(count(
                words.next(),
                "milliseconds",
            )?))),
            "powercycle" => Self::PowerCycle,
            "state" => Self::State,
            "quit" => Self::Quit,
            other => return fail(format!("unknown command: {other}")),
        };
        if let Some(extra) = words.next() {
            return fail(format!("unexpected: {extra}"));
        }
        Ok(command)
    }
}

/// What the caller still has to do after a command ran.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Print `ok`.
    Done,
    /// Print these lines (`state`).
    Text(String),
    /// Close the pty (`close`); only the binary can.
    Close,
    /// Open a new pty and print its path (`reopen`).
    Reopen,
    /// Exit (`quit`).
    Quit,
}

impl SimDevice {
    /// Runs one control command. The pty commands come back as an [`Outcome`]
    /// for the binary to carry out.
    pub fn apply(&mut self, command: Command) -> Outcome {
        match command {
            Command::Load(load) => self.set_load(load),
            Command::Vin(volts) => self.set_input_voltage(volts),
            Command::Temp(celsius) => self.set_temperature(celsius),
            Command::Trip(protection) => self.trip(protection),
            Command::Keep(id, value) => self.keep(id, value),
            Command::Unplug => self.unplug(),
            Command::Plug => self.plug(),
            Command::Corrupt(frames) => self.corrupt(frames),
            Command::Split(frames) => self.split(frames),
            Command::Delay(delay) => self.delay(delay),
            Command::PowerCycle => self.powercycle(),
            Command::State => return Outcome::Text(self.state()),
            Command::Close => return Outcome::Close,
            Command::Reopen => return Outcome::Reopen,
            Command::Quit => return Outcome::Quit,
        }
        Outcome::Done
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(line: &str) -> Result<Command, String> {
        line.parse().map_err(|error: ParseError| error.to_string())
    }

    #[test]
    fn sim_11_every_command_of_the_table_parses() {
        assert_eq!(parse("load 4.7"), Ok(Command::Load(Load::Ohms(4.7))));
        assert_eq!(parse("load open"), Ok(Command::Load(Load::Open)));
        assert_eq!(parse("vin 15"), Ok(Command::Vin(15.0)));
        assert_eq!(parse("temp 85"), Ok(Command::Temp(85.0)));
        assert_eq!(parse("trip ovp"), Ok(Command::Trip(Protection::Ovp)));
        assert_eq!(parse("trip lvp"), Ok(Command::Trip(Protection::Lvp)));
        assert_eq!(parse("keep C1 3.3"), Ok(Command::Keep(0xC1, 3.3)));
        assert_eq!(parse("keep 0xd6 2"), Ok(Command::Keep(0xD6, 2.0)));
        assert_eq!(parse("unplug"), Ok(Command::Unplug));
        assert_eq!(parse("plug"), Ok(Command::Plug));
        assert_eq!(parse("close"), Ok(Command::Close));
        assert_eq!(parse("reopen"), Ok(Command::Reopen));
        assert_eq!(parse("corrupt 3"), Ok(Command::Corrupt(3)));
        assert_eq!(parse("split 2"), Ok(Command::Split(2)));
        assert_eq!(
            parse("delay 100"),
            Ok(Command::Delay(Duration::from_millis(100)))
        );
        assert_eq!(parse("powercycle"), Ok(Command::PowerCycle));
        assert_eq!(parse("state"), Ok(Command::State));
        assert_eq!(parse("quit"), Ok(Command::Quit));
    }

    #[test]
    fn sim_11_bad_lines_say_why() {
        assert!(parse("load").unwrap_err().contains("missing ohms"));
        assert!(parse("load -1").unwrap_err().contains("positive"));
        assert!(parse("vin x").unwrap_err().contains("not a number"));
        assert!(
            parse("trip heat")
                .unwrap_err()
                .contains("unknown protection")
        );
        assert!(
            parse("keep C3 1")
                .unwrap_err()
                .contains("cannot be written")
        );
        assert!(parse("keep zz 1").unwrap_err().contains("hex byte"));
        assert!(parse("corrupt -1").unwrap_err().contains("whole number"));
        assert!(parse("state now").unwrap_err().contains("unexpected"));
        assert!(parse("dance").unwrap_err().contains("unknown command"));
    }
}
