//! The parameter (`type`) table of PROTOCOL.md §4.

use crate::frame::{Dir, command};

/// A preset memory slot, M1 to M6.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Slot(u8);

impl Slot {
    pub const COUNT: usize = 6;

    /// `number` is 1 for M1 up to 6 for M6.
    pub fn new(number: u8) -> Option<Self> {
        (1..=Self::COUNT as u8)
            .contains(&number)
            .then_some(Self(number))
    }

    pub fn number(self) -> u8 {
        self.0
    }
}

/// A readable parameter. Names follow PROTOCOL.md §4.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Register {
    InputVoltage,
    SetVoltage,
    SetCurrent,
    Output,
    Temperature,
    PresetVoltage(Slot),
    PresetCurrent(Slot),
    Ovp,
    Ocp,
    Opp,
    Otp,
    Lvp,
    Brightness,
    Volume,
    FlagD8,
    Capacity,
    Energy,
    OutputState,
    Protection,
    Mode,
    Model,
    HardwareVersion,
    FirmwareVersion,
    FlagE1,
    MaxVoltage,
    MaxCurrent,
    OvpRange,
    OcpRange,
    OppRange,
    OtpRange,
    LvpRange,
    Dump,
}

impl Register {
    /// The `type` byte.
    pub fn id(self) -> u8 {
        match self {
            Self::InputVoltage => 0xC0,
            Self::SetVoltage => 0xC1,
            Self::SetCurrent => 0xC2,
            Self::Output => 0xC3,
            Self::Temperature => 0xC4,
            Self::PresetVoltage(slot) => 0xC5 + 2 * (slot.number() - 1),
            Self::PresetCurrent(slot) => 0xC6 + 2 * (slot.number() - 1),
            Self::Ovp => 0xD1,
            Self::Ocp => 0xD2,
            Self::Opp => 0xD3,
            Self::Otp => 0xD4,
            Self::Lvp => 0xD5,
            Self::Brightness => 0xD6,
            Self::Volume => 0xD7,
            Self::FlagD8 => 0xD8,
            Self::Capacity => 0xD9,
            Self::Energy => 0xDA,
            Self::OutputState => 0xDB,
            Self::Protection => 0xDC,
            Self::Mode => 0xDD,
            Self::Model => 0xDE,
            Self::HardwareVersion => 0xDF,
            Self::FirmwareVersion => 0xE0,
            Self::FlagE1 => 0xE1,
            Self::MaxVoltage => 0xE2,
            Self::MaxCurrent => 0xE3,
            Self::OvpRange => 0xE4,
            Self::OcpRange => 0xE5,
            Self::OppRange => 0xE6,
            Self::OtpRange => 0xE7,
            Self::LvpRange => 0xE8,
            Self::Dump => 0xFF,
        }
    }

    pub fn from_id(id: u8) -> Option<Self> {
        Some(match id {
            0xC0 => Self::InputVoltage,
            0xC1 => Self::SetVoltage,
            0xC2 => Self::SetCurrent,
            0xC3 => Self::Output,
            0xC4 => Self::Temperature,
            0xC5..=0xD0 => {
                let slot = Slot::new((id - 0xC5) / 2 + 1)?;
                if (id - 0xC5).is_multiple_of(2) {
                    Self::PresetVoltage(slot)
                } else {
                    Self::PresetCurrent(slot)
                }
            }
            0xD1 => Self::Ovp,
            0xD2 => Self::Ocp,
            0xD3 => Self::Opp,
            0xD4 => Self::Otp,
            0xD5 => Self::Lvp,
            0xD6 => Self::Brightness,
            0xD7 => Self::Volume,
            0xD8 => Self::FlagD8,
            0xD9 => Self::Capacity,
            0xDA => Self::Energy,
            0xDB => Self::OutputState,
            0xDC => Self::Protection,
            0xDD => Self::Mode,
            0xDE => Self::Model,
            0xDF => Self::HardwareVersion,
            0xE0 => Self::FirmwareVersion,
            0xE1 => Self::FlagE1,
            0xE2 => Self::MaxVoltage,
            0xE3 => Self::MaxCurrent,
            0xE4 => Self::OvpRange,
            0xE5 => Self::OcpRange,
            0xE6 => Self::OppRange,
            0xE7 => Self::OtpRange,
            0xE8 => Self::LvpRange,
            0xFF => Self::Dump,
            _ => return None,
        })
    }

    /// Data bytes in a device reply; `None` for the text registers, which vary.
    pub fn data_len(self) -> Option<usize> {
        match self {
            Self::Output => Some(12),
            Self::Brightness
            | Self::Volume
            | Self::FlagD8
            | Self::OutputState
            | Self::Protection
            | Self::Mode
            | Self::FlagE1 => Some(1),
            Self::Model | Self::HardwareVersion | Self::FirmwareVersion => None,
            Self::Dump => Some(crate::DUMP_LEN),
            _ => Some(4),
        }
    }
}

/// The data length a frame of a known kind must have (APP-03), if it has one.
pub(crate) fn expected_len(dir: Dir, command: u8, id: u8) -> Option<usize> {
    match (dir, command) {
        (Dir::DeviceToHost, command::DATA) => Register::from_id(id)?.data_len(),
        (Dir::HostToDevice, command::DATA | command::CONNECT) => Some(1),
        (Dir::HostToDevice, command::SET) => Register::from_id(id)?.data_len(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prt_02_every_listed_type_round_trips_and_nothing_else_is_known() {
        let known: Vec<u8> = (0..=u8::MAX)
            .filter(|id| Register::from_id(*id).is_some())
            .collect();
        let listed: Vec<u8> = (0xC0..=0xE8).chain([0xFF]).collect();
        assert_eq!(known, listed);
        for id in known {
            assert_eq!(Register::from_id(id).unwrap().id(), id);
        }
    }

    #[test]
    fn prt_02_preset_types_pair_voltage_then_current_per_slot() {
        let m1 = Slot::new(1).unwrap();
        let m6 = Slot::new(6).unwrap();
        assert_eq!(Register::from_id(0xC5), Some(Register::PresetVoltage(m1)));
        assert_eq!(Register::from_id(0xC6), Some(Register::PresetCurrent(m1)));
        assert_eq!(Register::from_id(0xCF), Some(Register::PresetVoltage(m6)));
        assert_eq!(Register::from_id(0xD0), Some(Register::PresetCurrent(m6)));
        assert_eq!(Slot::new(0), None);
        assert_eq!(Slot::new(7), None);
    }
}
