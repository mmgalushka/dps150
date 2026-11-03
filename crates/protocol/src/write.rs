//! The writes the app may send (PROTOCOL.md §4): setpoints, preset slots and
//! protection limits. There is deliberately no variant for the output switch,
//! brightness or volume; those arrive with their own milestones (PLAN.md §3).
//! Values are the integer types of [`crate::value`], so a negative or
//! non-finite number cannot be built (LIM-01, PRT-04).

use crate::frame::{Dir, Frame, command};
use crate::register::{Register, Slot};
use crate::value::{MilliCelsius, Milliamps, Millivolts, Milliwatts};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Write {
    SetVoltage(Millivolts),
    SetCurrent(Milliamps),
    PresetVoltage(Slot, Millivolts),
    PresetCurrent(Slot, Milliamps),
    Ovp(Millivolts),
    Ocp(Milliamps),
    Opp(Milliwatts),
    Otp(MilliCelsius),
    Lvp(Millivolts),
}

impl Write {
    /// The register this writes; also the key under which writes coalesce.
    pub fn register(self) -> Register {
        match self {
            Self::SetVoltage(_) => Register::SetVoltage,
            Self::SetCurrent(_) => Register::SetCurrent,
            Self::PresetVoltage(slot, _) => Register::PresetVoltage(slot),
            Self::PresetCurrent(slot, _) => Register::PresetCurrent(slot),
            Self::Ovp(_) => Register::Ovp,
            Self::Ocp(_) => Register::Ocp,
            Self::Opp(_) => Register::Opp,
            Self::Otp(_) => Register::Otp,
            Self::Lvp(_) => Register::Lvp,
        }
    }

    /// The value in thousandths of its unit.
    pub fn thousandths(self) -> u32 {
        match self {
            Self::SetVoltage(v) | Self::PresetVoltage(_, v) | Self::Ovp(v) | Self::Lvp(v) => {
                v.get()
            }
            Self::SetCurrent(a) | Self::PresetCurrent(_, a) | Self::Ocp(a) => a.get(),
            Self::Opp(w) => w.get(),
            Self::Otp(t) => t.get(),
        }
    }

    /// The SET frame: `F1 B1 <type> 04 <f32 LE> <cksum>`.
    pub fn to_frame(self) -> Frame {
        let wire = match self {
            Self::SetVoltage(v) | Self::PresetVoltage(_, v) | Self::Ovp(v) | Self::Lvp(v) => {
                v.to_wire()
            }
            Self::SetCurrent(a) | Self::PresetCurrent(_, a) | Self::Ocp(a) => a.to_wire(),
            Self::Opp(w) => w.to_wire(),
            Self::Otp(t) => t.to_wire(),
        };
        Frame::new(
            Dir::HostToDevice,
            command::SET,
            self.register().id(),
            wire.to_le_bytes(),
        )
        .expect("four bytes fit")
    }

    /// The display resolution in thousandths: the smallest difference the
    /// screens show (10 mV, 1 mA, 0.1 W, 0.1 °C; DESIGN.md §4.3).
    pub fn resolution(self) -> u32 {
        match self {
            Self::SetVoltage(_) | Self::PresetVoltage(..) | Self::Ovp(_) | Self::Lvp(_) => 10,
            Self::SetCurrent(_) | Self::PresetCurrent(..) | Self::Ocp(_) => 1,
            Self::Opp(_) | Self::Otp(_) => 100,
        }
    }

    /// Does `other` (same register) differ from `self` by more than the
    /// display resolution? That is the "Device kept" test (PRT-03).
    pub fn differs_from(self, other: Self) -> bool {
        self.thousandths().abs_diff(other.thousandths()) > self.resolution() / 2
    }

    /// The same write with the value replaced by `thousandths`.
    pub fn with_thousandths(self, thousandths: u32) -> Self {
        match self {
            Self::SetVoltage(_) => Self::SetVoltage(Millivolts::new(thousandths)),
            Self::SetCurrent(_) => Self::SetCurrent(Milliamps::new(thousandths)),
            Self::PresetVoltage(slot, _) => Self::PresetVoltage(slot, Millivolts::new(thousandths)),
            Self::PresetCurrent(slot, _) => Self::PresetCurrent(slot, Milliamps::new(thousandths)),
            Self::Ovp(_) => Self::Ovp(Millivolts::new(thousandths)),
            Self::Ocp(_) => Self::Ocp(Milliamps::new(thousandths)),
            Self::Opp(_) => Self::Opp(Milliwatts::new(thousandths)),
            Self::Otp(_) => Self::Otp(MilliCelsius::new(thousandths)),
            Self::Lvp(_) => Self::Lvp(Millivolts::new(thousandths)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prt_04_a_set_frame_has_the_documented_layout() {
        // 5.0 V is 0x40A00000: bytes 00 00 A0 40; checksum C1+04+00+00+A0+40.
        let frame = Write::SetVoltage(Millivolts::new(5000)).to_frame();
        assert_eq!(
            frame.encode(),
            [0xF1, 0xB1, 0xC1, 0x04, 0x00, 0x00, 0xA0, 0x40, 0xA5]
        );
    }

    #[test]
    fn prt_04_every_write_goes_to_its_own_register() {
        let slot = Slot::new(3).unwrap();
        let cases = [
            (Write::SetCurrent(Milliamps::new(1500)), 0xC2),
            (Write::PresetVoltage(slot, Millivolts::new(9000)), 0xC9),
            (Write::PresetCurrent(slot, Milliamps::new(2000)), 0xCA),
            (Write::Ovp(Millivolts::new(30000)), 0xD1),
            (Write::Ocp(Milliamps::new(5100)), 0xD2),
            (Write::Opp(Milliwatts::new(150_000)), 0xD3),
            (Write::Otp(MilliCelsius::new(80_000)), 0xD4),
            (Write::Lvp(Millivolts::new(5000)), 0xD5),
        ];
        for (write, id) in cases {
            assert_eq!(write.register().id(), id);
            assert_eq!(write.to_frame().id(), id);
            assert_eq!(write.to_frame().command(), command::SET);
        }
    }

    #[test]
    fn prt_04_a_value_read_back_goes_out_with_the_same_bits() {
        // 3.3 V as the device reports it: 3.299999952.
        // The binary32 nearest to 3.3 prints as 3.3 but holds 3.299999952.
        let read = Millivolts::from_wire(f32::from_bits(0x4053_3333)).unwrap();
        assert_eq!(read.get(), 3300);
        let frame = Write::SetVoltage(read).to_frame();
        assert_eq!(frame.data(), 3.3f32.to_le_bytes());
    }

    #[test]
    fn prt_03_a_difference_is_one_the_display_can_show() {
        let v = |mv| Write::SetVoltage(Millivolts::new(mv));
        assert!(!v(5000).differs_from(v(5000)));
        assert!(!v(5000).differs_from(v(5004)), "below half a step");
        assert!(v(5000).differs_from(v(5010)));
        assert!(v(5000).differs_from(v(4000)));
        let a = |ma| Write::SetCurrent(Milliamps::new(ma));
        assert!(!a(1500).differs_from(a(1500)));
        assert!(a(1500).differs_from(a(1501)));
    }

    #[test]
    fn prt_04_with_thousandths_keeps_the_register() {
        let write = Write::SetCurrent(Milliamps::new(1)).with_thousandths(2500);
        assert_eq!(write, Write::SetCurrent(Milliamps::new(2500)));
        assert_eq!(write.thousandths(), 2500);
    }
}
