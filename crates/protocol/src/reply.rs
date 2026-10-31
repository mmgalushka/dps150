//! Typed values of a device data frame.

use std::fmt;

use crate::dump::{Dump, Mode, OutputReading};
use crate::frame::{Dir, Frame, command};
use crate::register::{Register, Slot};
use crate::value::{MilliCelsius, Milliamps, Millivolts, Milliwatts, ValueError};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// Not an `A1` frame from the device.
    NotAReply,
    UnknownRegister(u8),
    WrongLength {
        field: &'static str,
        expected: usize,
        got: usize,
    },
    BadValue {
        field: &'static str,
        error: ValueError,
    },
    /// A flag byte that is neither of its documented values.
    BadFlag {
        field: &'static str,
        value: u8,
    },
    NotText {
        field: &'static str,
    },
}

impl fmt::Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAReply => f.write_str("not a data frame from the device"),
            Self::UnknownRegister(id) => write!(f, "unknown type {id:#04x}"),
            Self::WrongLength {
                field,
                expected,
                got,
            } => {
                write!(f, "{field}: {got} data bytes, expected {expected}")
            }
            Self::BadValue { field, error } => write!(f, "{field}: {error}"),
            Self::BadFlag { field, value } => write!(f, "{field}: unexpected value {value}"),
            Self::NotText { field } => write!(f, "{field}: not ASCII text"),
        }
    }
}

impl std::error::Error for DecodeError {}

/// A device data frame with typed values.
///
/// Setpoints, limits and ranges are integer thousandths; measured values stay
/// floats (PRT-04).
#[derive(Clone, Debug, PartialEq)]
pub enum Reply {
    InputVoltage(f32),
    SetVoltage(Millivolts),
    SetCurrent(Milliamps),
    Output(OutputReading),
    Temperature(f32),
    PresetVoltage(Slot, Millivolts),
    PresetCurrent(Slot, Milliamps),
    Ovp(Millivolts),
    Ocp(Milliamps),
    Opp(Milliwatts),
    Otp(MilliCelsius),
    Lvp(Millivolts),
    Brightness(u8),
    Volume(u8),
    FlagD8(u8),
    Capacity(f32),
    Energy(f32),
    OutputState(bool),
    Protection(u8),
    Mode(Mode),
    Model(String),
    HardwareVersion(String),
    FirmwareVersion(String),
    FlagE1(u8),
    MaxVoltage(Millivolts),
    MaxCurrent(Milliamps),
    OvpRange(Millivolts),
    OcpRange(Milliamps),
    OppRange(Milliwatts),
    OtpRange(MilliCelsius),
    LvpRange(Millivolts),
    Dump(Box<Dump>),
}

impl Reply {
    pub fn from_frame(frame: &Frame) -> Result<Self, DecodeError> {
        if frame.dir() != Dir::DeviceToHost || frame.command() != command::DATA {
            return Err(DecodeError::NotAReply);
        }
        let register = frame
            .register()
            .ok_or(DecodeError::UnknownRegister(frame.id()))?;
        let data = frame.data();
        let name = name(register);
        if let Some(expected) = register.data_len()
            && data.len() != expected
        {
            return Err(DecodeError::WrongLength {
                field: name,
                expected,
                got: data.len(),
            });
        }
        Ok(match register {
            Register::InputVoltage => Self::InputVoltage(float(data)),
            Register::SetVoltage => Self::SetVoltage(value(name, data)?),
            Register::SetCurrent => Self::SetCurrent(value(name, data)?),
            Register::Output => Self::Output(OutputReading::from_bytes(data)),
            Register::Temperature => Self::Temperature(float(data)),
            Register::PresetVoltage(slot) => Self::PresetVoltage(slot, value(name, data)?),
            Register::PresetCurrent(slot) => Self::PresetCurrent(slot, value(name, data)?),
            Register::Ovp => Self::Ovp(value(name, data)?),
            Register::Ocp => Self::Ocp(value(name, data)?),
            Register::Opp => Self::Opp(value(name, data)?),
            Register::Otp => Self::Otp(value(name, data)?),
            Register::Lvp => Self::Lvp(value(name, data)?),
            Register::Brightness => Self::Brightness(data[0]),
            Register::Volume => Self::Volume(data[0]),
            Register::FlagD8 => Self::FlagD8(data[0]),
            Register::Capacity => Self::Capacity(float(data)),
            Register::Energy => Self::Energy(float(data)),
            Register::OutputState => Self::OutputState(output_state(data[0])?),
            Register::Protection => Self::Protection(data[0]),
            Register::Mode => Self::Mode(Mode::from_byte(data[0])?),
            Register::Model => Self::Model(text(name, data)?),
            Register::HardwareVersion => Self::HardwareVersion(text(name, data)?),
            Register::FirmwareVersion => Self::FirmwareVersion(text(name, data)?),
            Register::FlagE1 => Self::FlagE1(data[0]),
            Register::MaxVoltage => Self::MaxVoltage(value(name, data)?),
            Register::MaxCurrent => Self::MaxCurrent(value(name, data)?),
            Register::OvpRange => Self::OvpRange(value(name, data)?),
            Register::OcpRange => Self::OcpRange(value(name, data)?),
            Register::OppRange => Self::OppRange(value(name, data)?),
            Register::OtpRange => Self::OtpRange(value(name, data)?),
            Register::LvpRange => Self::LvpRange(value(name, data)?),
            Register::Dump => Self::Dump(Box::new(Dump::from_bytes(data)?)),
        })
    }
}

/// A little-endian binary32 at the start of `data`.
pub(crate) fn float(data: &[u8]) -> f32 {
    f32::from_le_bytes([data[0], data[1], data[2], data[3]])
}

pub(crate) trait FromWire: Sized {
    fn from_wire(value: f32) -> Result<Self, ValueError>;
}

macro_rules! from_wire {
    ($($ty:ty),*) => {$(
        impl FromWire for $ty {
            fn from_wire(value: f32) -> Result<Self, ValueError> {
                <$ty>::from_wire(value)
            }
        }
    )*};
}
from_wire!(Millivolts, Milliamps, Milliwatts, MilliCelsius);

pub(crate) fn value<T: FromWire>(field: &'static str, data: &[u8]) -> Result<T, DecodeError> {
    T::from_wire(float(data)).map_err(|error| DecodeError::BadValue { field, error })
}

pub(crate) fn output_state(byte: u8) -> Result<bool, DecodeError> {
    match byte {
        0 => Ok(false),
        1 => Ok(true),
        value => Err(DecodeError::BadFlag {
            field: "output state",
            value,
        }),
    }
}

fn text(field: &'static str, data: &[u8]) -> Result<String, DecodeError> {
    if !data.is_ascii() {
        return Err(DecodeError::NotText { field });
    }
    Ok(String::from_utf8_lossy(data).into_owned())
}

fn name(register: Register) -> &'static str {
    match register {
        Register::InputVoltage => "input voltage",
        Register::SetVoltage => "set voltage",
        Register::SetCurrent => "set current",
        Register::Output => "output",
        Register::Temperature => "temperature",
        Register::PresetVoltage(_) => "preset voltage",
        Register::PresetCurrent(_) => "preset current",
        Register::Ovp => "OVP",
        Register::Ocp => "OCP",
        Register::Opp => "OPP",
        Register::Otp => "OTP",
        Register::Lvp => "LVP",
        Register::Brightness => "brightness",
        Register::Volume => "volume",
        Register::FlagD8 => "flag D8",
        Register::Capacity => "capacity",
        Register::Energy => "energy",
        Register::OutputState => "output state",
        Register::Protection => "protection state",
        Register::Mode => "mode",
        Register::Model => "model",
        Register::HardwareVersion => "hardware version",
        Register::FirmwareVersion => "firmware version",
        Register::FlagE1 => "flag E1",
        Register::MaxVoltage => "live max voltage",
        Register::MaxCurrent => "max current",
        Register::OvpRange => "OVP range",
        Register::OcpRange => "OCP range",
        Register::OppRange => "OPP range",
        Register::OtpRange => "OTP range",
        Register::LvpRange => "LVP range",
        Register::Dump => "full dump",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device_frame(register: Register, data: &[u8]) -> Frame {
        Frame::new(Dir::DeviceToHost, command::DATA, register.id(), data).unwrap()
    }

    #[test]
    fn prt_02_pushed_input_voltage_decodes() {
        // PROTOCOL.md §9: F0 A1 C0 04 B5 F8 A1 41 = input 20.246 V.
        let frame = device_frame(Register::InputVoltage, &[0xB5, 0xF8, 0xA1, 0x41]);
        let Reply::InputVoltage(volts) = Reply::from_frame(&frame).unwrap() else {
            panic!()
        };
        assert!((volts - 20.246).abs() < 1e-3);
    }

    #[test]
    fn prt_02_model_name_is_text_of_any_length() {
        let frame = device_frame(Register::Model, b"DPS-150");
        assert_eq!(
            Reply::from_frame(&frame),
            Ok(Reply::Model("DPS-150".into()))
        );
        let frame = device_frame(Register::Model, &[0xFF]);
        assert_eq!(
            Reply::from_frame(&frame),
            Err(DecodeError::NotText { field: "model" })
        );
    }

    #[test]
    fn prt_02_wrong_length_and_unknown_type_are_errors_not_panics() {
        let frame = device_frame(Register::Temperature, &[0; 3]);
        assert_eq!(
            Reply::from_frame(&frame),
            Err(DecodeError::WrongLength {
                field: "temperature",
                expected: 4,
                got: 3
            })
        );
        let frame = Frame::new(Dir::DeviceToHost, command::DATA, 0x42, [0]).unwrap();
        assert_eq!(
            Reply::from_frame(&frame),
            Err(DecodeError::UnknownRegister(0x42))
        );
        assert_eq!(
            Reply::from_frame(&Frame::connect()),
            Err(DecodeError::NotAReply)
        );
    }

    #[test]
    fn lim_01_a_negative_setpoint_from_the_device_is_refused() {
        // PROTOCOL.md §7: the device stores -1.0 V verbatim.
        let frame = device_frame(Register::SetVoltage, &(-1.0_f32).to_le_bytes());
        assert_eq!(
            Reply::from_frame(&frame),
            Err(DecodeError::BadValue {
                field: "set voltage",
                error: ValueError::Negative
            })
        );
    }

    #[test]
    fn prt_02_output_state_is_zero_or_one() {
        assert_eq!(
            Reply::from_frame(&device_frame(Register::OutputState, &[1])),
            Ok(Reply::OutputState(true))
        );
        assert_eq!(
            Reply::from_frame(&device_frame(Register::OutputState, &[0])),
            Ok(Reply::OutputState(false))
        );
        assert_eq!(
            Reply::from_frame(&device_frame(Register::OutputState, &[2])),
            Err(DecodeError::BadFlag {
                field: "output state",
                value: 2
            })
        );
    }
}
