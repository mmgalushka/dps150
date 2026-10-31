//! The full state dump, `GET FF` (PROTOCOL.md §6).

use crate::reply::{DecodeError, float, output_state, value};
use crate::value::{MilliCelsius, Milliamps, Millivolts, Milliwatts};

/// Data bytes in the dump.
pub const DUMP_LEN: usize = 139;

/// Regulation mode, register `DD`. CV is 1 [V]; CC is 0 from community
/// sources [U] (PROTOCOL.md §4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    ConstantVoltage,
    ConstantCurrent,
}

impl Mode {
    pub(crate) fn from_byte(byte: u8) -> Result<Self, DecodeError> {
        match byte {
            1 => Ok(Self::ConstantVoltage),
            0 => Ok(Self::ConstantCurrent),
            value => Err(DecodeError::BadFlag {
                field: "mode",
                value,
            }),
        }
    }
}

/// Measured output, register `C3`. Only the all-zero case has been seen; the
/// V, I, P order comes from community sources [U].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct OutputReading {
    pub voltage: f32,
    pub current: f32,
    pub power: f32,
}

impl OutputReading {
    pub(crate) fn from_bytes(data: &[u8]) -> Self {
        Self {
            voltage: float(&data[0..]),
            current: float(&data[4..]),
            power: float(&data[8..]),
        }
    }
}

/// One preset memory slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Preset {
    pub voltage: Millivolts,
    pub current: Milliamps,
}

/// Everything one `GET FF` returns.
#[derive(Clone, Debug, PartialEq)]
pub struct Dump {
    pub input_voltage: f32,
    pub set_voltage: Millivolts,
    pub set_current: Milliamps,
    pub output: OutputReading,
    pub temperature: f32,
    pub presets: [Preset; 6],
    pub ovp: Millivolts,
    pub ocp: Milliamps,
    pub opp: Milliwatts,
    pub otp: MilliCelsius,
    pub lvp: Millivolts,
    pub brightness: u8,
    pub volume: u8,
    pub flag_d8: u8,
    pub capacity_ah: f32,
    pub energy_wh: f32,
    pub output_on: bool,
    /// Raw `DC`; the mapping to OVP, OCP and the rest is unverified [U].
    pub protection: u8,
    pub mode: Mode,
    pub flag_e1: u8,
    pub max_voltage: Millivolts,
    pub max_current: Milliamps,
    pub ovp_range: Millivolts,
    pub ocp_range: Milliamps,
    pub opp_range: Milliwatts,
    pub otp_range: MilliCelsius,
    pub lvp_range: Millivolts,
}

impl Dump {
    pub(crate) fn from_bytes(data: &[u8]) -> Result<Self, DecodeError> {
        debug_assert_eq!(data.len(), DUMP_LEN);
        let at = |offset: usize| &data[offset..];
        let mut presets = [Preset {
            voltage: Millivolts::ZERO,
            current: Milliamps::ZERO,
        }; 6];
        for (slot, preset) in presets.iter_mut().enumerate() {
            *preset = Preset {
                voltage: value("preset voltage", at(28 + 8 * slot))?,
                current: value("preset current", at(32 + 8 * slot))?,
            };
        }
        Ok(Self {
            input_voltage: float(at(0)),
            set_voltage: value("set voltage", at(4))?,
            set_current: value("set current", at(8))?,
            output: OutputReading::from_bytes(at(12)),
            temperature: float(at(24)),
            presets,
            ovp: value("OVP", at(76))?,
            ocp: value("OCP", at(80))?,
            opp: value("OPP", at(84))?,
            otp: value("OTP", at(88))?,
            lvp: value("LVP", at(92))?,
            brightness: data[96],
            volume: data[97],
            flag_d8: data[98],
            capacity_ah: float(at(99)),
            energy_wh: float(at(103)),
            output_on: output_state(data[107])?,
            protection: data[108],
            mode: Mode::from_byte(data[109])?,
            flag_e1: data[110],
            max_voltage: value("live max voltage", at(111))?,
            max_current: value("max current", at(115))?,
            ovp_range: value("OVP range", at(119))?,
            ocp_range: value("OCP range", at(123))?,
            opp_range: value("OPP range", at(127))?,
            otp_range: value("OTP range", at(131))?,
            lvp_range: value("LVP range", at(135))?,
        })
    }
}
