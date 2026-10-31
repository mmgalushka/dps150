//! Integer thousandths of a unit (PRT-04).
//!
//! The app holds setpoints, limits and presets as whole thousandths and
//! converts to binary32 only at the wire. The types are unsigned, so a negative
//! value cannot be expressed, and [`from_wire`](Millivolts::from_wire) refuses
//! NaN and infinity (LIM-01).

use std::fmt;

/// Why a wire float is not a valid value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueError {
    NotFinite,
    Negative,
    TooLarge,
}

impl fmt::Display for ValueError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotFinite => "not a finite number",
            Self::Negative => "negative",
            Self::TooLarge => "too large",
        })
    }
}

impl std::error::Error for ValueError {}

macro_rules! thousandths {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u32);

        impl $name {
            pub const ZERO: Self = Self(0);

            pub const fn new(thousandths: u32) -> Self {
                Self(thousandths)
            }

            pub const fn get(self) -> u32 {
                self.0
            }

            /// The nearest whole thousandth of a float read from the device.
            pub fn from_wire(value: f32) -> Result<Self, ValueError> {
                if !value.is_finite() {
                    return Err(ValueError::NotFinite);
                }
                if value < 0.0 {
                    return Err(ValueError::Negative);
                }
                let thousandths = (f64::from(value) * 1000.0).round();
                if thousandths > f64::from(u32::MAX) {
                    return Err(ValueError::TooLarge);
                }
                Ok(Self(thousandths as u32))
            }

            /// The nearest binary32 to this value, as it goes on the wire.
            pub fn to_wire(self) -> f32 {
                (f64::from(self.0) / 1000.0) as f32
            }
        }
    };
}

thousandths!(
    /// Volts in thousandths (mV).
    Millivolts
);
thousandths!(
    /// Amperes in thousandths (mA).
    Milliamps
);
thousandths!(
    /// Watts in thousandths (mW).
    Milliwatts
);
thousandths!(
    /// Degrees Celsius in thousandths (m°C).
    MilliCelsius
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prt_04_device_float_becomes_whole_thousandths() {
        // PROTOCOL.md §7: 3.3 V is stored as the binary32 3.299999952.
        assert_eq!(Millivolts::from_wire(3.3), Ok(Millivolts::new(3300)));
        assert_eq!(Milliamps::from_wire(5.1), Ok(Milliamps::new(5100)));
        assert_eq!(Milliwatts::from_wire(150.0), Ok(Milliwatts::new(150_000)));
        assert_eq!(MilliCelsius::from_wire(99.0), Ok(MilliCelsius::new(99_000)));
    }

    #[test]
    fn prt_04_value_read_and_written_back_keeps_its_bits() {
        for thousandths in 0..=1_000_000_u32 {
            let wire = (f64::from(thousandths) / 1000.0) as f32;
            let value = Millivolts::from_wire(wire).unwrap();
            assert_eq!(value.get(), thousandths);
            assert_eq!(value.to_wire().to_bits(), wire.to_bits());
        }
    }

    #[test]
    fn lim_01_negative_nan_and_infinite_values_are_refused() {
        for bad in [-1.0, -0.000_1, f32::MIN] {
            assert_eq!(Millivolts::from_wire(bad), Err(ValueError::Negative));
        }
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(Millivolts::from_wire(bad), Err(ValueError::NotFinite));
        }
        assert_eq!(Millivolts::from_wire(f32::MAX), Err(ValueError::TooLarge));
    }

    #[test]
    fn lim_01_negative_zero_is_zero() {
        assert_eq!(Milliamps::from_wire(-0.0), Ok(Milliamps::ZERO));
    }
}
