//! Number formats (DESIGN.md §4.3, UI-07). Setpoints are integer thousandths;
//! measured values are floats rounded for display only.

use protocol::value::{Milliamps, Millivolts};

/// Set voltage, big: `DD.DD` with the leading zero kept.
pub fn set_voltage(value: Millivolts) -> String {
    let hundredths = (value.get() + 5) / 10;
    format!("{:02}.{:02}", hundredths / 100, hundredths % 100)
}

/// Set current, big: `D.DDD`.
pub fn set_current(value: Milliamps) -> String {
    let milli = value.get();
    format!("{}.{:03}", milli / 1000, milli % 1000)
}

/// Measured voltage, big: three decimals, no leading zero. In Compact it has
/// two decimals from 10 V so it fits 17 cells.
pub fn measured_voltage(volts: f32, compact: bool) -> String {
    let volts = clean(volts);
    if compact && volts >= 10.0 {
        format!("{volts:.2}")
    } else {
        format!("{volts:.3}")
    }
}

pub fn measured_current(amps: f32) -> String {
    format!("{:.3}", clean(amps))
}

/// Power: `D.DD W` below 100 W, `DDD.D W` from 100 W.
pub fn power(watts: f32) -> String {
    let watts = clean(watts);
    if watts < 99.995 {
        format!("{watts:.2} W")
    } else {
        format!("{watts:.1} W")
    }
}

pub fn input_voltage(volts: f32) -> String {
    format!("{:.2} V", clean(volts))
}

pub fn temperature(celsius: f32) -> String {
    format!("{celsius:.1} °C")
}

pub fn capacity(amp_hours: f32) -> String {
    format!("{:.3} Ah", clean(amp_hours))
}

pub fn energy(watt_hours: f32) -> String {
    format!("{:.3} Wh", clean(watt_hours))
}

/// Shortest decimal, like `%g`: 3.3, 12, 0.5.
pub fn shortest(thousandths: u32) -> String {
    let text = format!("{}.{:03}", thousandths / 1000, thousandths % 1000);
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// Preset on the Standard row: `3.3/1`.
pub fn preset_short(volts: Millivolts, amps: Milliamps) -> String {
    format!("{}/{}", shortest(volts.get()), shortest(amps.get()))
}

/// Preset in the Large box: fixed one decimal, `3.3V/1.0A`.
pub fn preset_long(volts: Millivolts, amps: Milliamps) -> String {
    let tenth = |value: u32| format!("{}.{}", (value + 50) / 1000, ((value + 50) % 1000) / 100);
    format!("{}V/{}A", tenth(volts.get()), tenth(amps.get()))
}

/// A reading that is not a finite non-negative number shows as zero rather
/// than as garbage digits.
fn clean(value: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ui_07_set_values_keep_the_leading_zero() {
        assert_eq!(set_voltage(Millivolts::new(5000)), "05.00");
        assert_eq!(set_voltage(Millivolts::new(30000)), "30.00");
        assert_eq!(set_voltage(Millivolts::new(3300)), "03.30");
        assert_eq!(set_current(Milliamps::new(1500)), "1.500");
        assert_eq!(set_current(Milliamps::new(5100)), "5.100");
        assert_eq!(set_current(Milliamps::new(7)), "0.007");
    }

    #[test]
    fn ui_07_measured_voltage_has_two_decimals_only_in_compact_from_ten_volts() {
        assert_eq!(measured_voltage(4.998, false), "4.998");
        assert_eq!(measured_voltage(4.998, true), "4.998");
        assert_eq!(measured_voltage(12.3456, false), "12.346");
        assert_eq!(measured_voltage(12.3456, true), "12.35");
        assert_eq!(measured_voltage(0.0, true), "0.000");
        assert_eq!(measured_current(0.512), "0.512");
    }

    #[test]
    fn ui_07_power_changes_format_at_one_hundred_watts() {
        assert_eq!(power(2.56), "2.56 W");
        assert_eq!(power(99.9), "99.90 W");
        assert_eq!(power(100.0), "100.0 W");
        assert_eq!(power(149.96), "150.0 W");
    }

    #[test]
    fn ui_07_other_values_and_bad_readings() {
        assert_eq!(input_voltage(20.25), "20.25 V");
        assert_eq!(temperature(24.3), "24.3 °C");
        assert_eq!(capacity(0.012), "0.012 Ah");
        assert_eq!(energy(0.06), "0.060 Wh");
        assert_eq!(measured_voltage(f32::NAN, false), "0.000");
        assert_eq!(measured_current(-1.0), "0.000");
    }

    #[test]
    fn ui_07_preset_shorthand_and_fixed_forms() {
        let (v, i) = (Millivolts::new(3300), Milliamps::new(1000));
        assert_eq!(preset_short(v, i), "3.3/1");
        assert_eq!(preset_long(v, i), "3.3V/1.0A");
        let (v, i) = (Millivolts::new(12000), Milliamps::new(2000));
        assert_eq!(preset_short(v, i), "12/2");
        assert_eq!(preset_long(v, i), "12.0V/2.0A");
        let (v, i) = (Millivolts::new(5000), Milliamps::new(500));
        assert_eq!(preset_short(v, i), "5/0.5");
        assert_eq!(preset_long(v, i), "5.0V/0.5A");
    }
}
