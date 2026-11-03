//! The five protection limits: values, defaults, digit layout and the
//! validation of a candidate (DESIGN.md §8.3, SAFETY.md §1).

use protocol::value::{MilliCelsius, Milliamps, Millivolts, Milliwatts};
use protocol::{Dump, Write};

use crate::constants::{LIMIT_MARGIN_MA, LIMIT_MARGIN_MV, LIMIT_MARGIN_MW};
use crate::limits::{one_decimal, two_decimals};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Limit {
    Ovp,
    Ocp,
    Opp,
    Otp,
    Lvp,
}

pub const ALL: [Limit; 5] = [Limit::Ovp, Limit::Ocp, Limit::Opp, Limit::Otp, Limit::Lvp];

impl Limit {
    pub fn name(self) -> &'static str {
        match self {
            Self::Ovp => "OVP",
            Self::Ocp => "OCP",
            Self::Opp => "OPP",
            Self::Otp => "OTP",
            Self::Lvp => "LVP",
        }
    }

    pub fn index(self) -> usize {
        ALL.iter().position(|l| *l == self).unwrap_or(0)
    }

    /// Device default, observed on a fresh unit (PROTOCOL.md §4).
    pub fn default_value(self) -> u32 {
        match self {
            Self::Ovp => 30_000,
            Self::Ocp => 5_100,
            Self::Opp => 150_000,
            Self::Otp => 80_000,
            Self::Lvp => 5_000,
        }
    }

    /// The limit as the device holds it.
    pub fn value(self, dump: &Dump) -> u32 {
        match self {
            Self::Ovp => dump.ovp.get(),
            Self::Ocp => dump.ocp.get(),
            Self::Opp => dump.opp.get(),
            Self::Otp => dump.otp.get(),
            Self::Lvp => dump.lvp.get(),
        }
    }

    /// The device range maximum (`E4`..`E8`).
    pub fn range_max(self, dump: &Dump) -> u32 {
        match self {
            Self::Ovp => dump.ovp_range.get(),
            Self::Ocp => dump.ocp_range.get(),
            Self::Opp => dump.opp_range.get(),
            Self::Otp => dump.otp_range.get(),
            Self::Lvp => dump.lvp_range.get(),
        }
    }

    pub fn write(self, value: u32) -> Write {
        match self {
            Self::Ovp => Write::Ovp(Millivolts::new(value)),
            Self::Ocp => Write::Ocp(Milliamps::new(value)),
            Self::Opp => Write::Opp(Milliwatts::new(value)),
            Self::Otp => Write::Otp(MilliCelsius::new(value)),
            Self::Lvp => Write::Lvp(Millivolts::new(value)),
        }
    }

    /// Weights of the editable digits, left to right, in thousandths. The
    /// formats are `NN.NN`, `NN.NN`, `NNN.N`, `NN.N`, `NN.NN` (§4.3).
    pub fn weights(self) -> &'static [u32] {
        match self {
            Self::Ovp | Self::Ocp | Self::Lvp => &[10_000, 1_000, 100, 10],
            Self::Opp => &[100_000, 10_000, 1_000, 100],
            Self::Otp => &[10_000, 1_000, 100],
        }
    }

    /// The units digit, where the cursor starts.
    pub fn initial_digit(self) -> usize {
        match self {
            Self::Opp => 2,
            _ => 1,
        }
    }

    /// Big-digit text of a value, leading zeros kept (§4.3).
    pub fn digits(self, value: u32) -> String {
        match self {
            Self::Ovp | Self::Ocp | Self::Lvp => {
                let hundredths = (value + 5) / 10;
                format!("{:02}.{:02}", hundredths / 100, hundredths % 100)
            }
            Self::Opp => {
                let tenths = (value + 50) / 100;
                format!("{:03}.{}", tenths / 10, tenths % 10)
            }
            Self::Otp => {
                let tenths = (value + 50) / 100;
                format!("{:02}.{}", tenths / 10, tenths % 10)
            }
        }
    }

    /// Character position of editable digit `index` in `digits()`.
    pub fn char_of_digit(self, index: usize) -> usize {
        match self {
            // NN.NN
            Self::Ovp | Self::Ocp | Self::Lvp => [0, 1, 3, 4][index.min(3)],
            // NNN.N
            Self::Opp => [0, 1, 2, 4][index.min(3)],
            // NN.N
            Self::Otp => [0, 1, 3][index.min(2)],
        }
    }

    /// Table text: `30.00 V`, `5.10 A`, `150.0 W`, `80.0 °C`, `5.00 V`.
    pub fn text(self, value: u32) -> String {
        match self {
            Self::Ovp | Self::Lvp => format!("{} V", two_decimals(value)),
            Self::Ocp => format!("{} A", two_decimals(value)),
            Self::Opp => format!("{} W", one_decimal(value)),
            Self::Otp => format!("{} °C", one_decimal(value)),
        }
    }

    /// Device range text: `0 – 30 V`, `0 – 5.10 A`, `0 – 150 W`, `0 – 99 °C`.
    pub fn range_text(self, max: u32) -> String {
        let whole = |value: u32| {
            if value.is_multiple_of(1000) {
                format!("{}", value / 1000)
            } else {
                two_decimals(value)
            }
        };
        match self {
            Self::Ovp | Self::Lvp => format!("0 – {} V", whole(max)),
            Self::Ocp => format!("0 – {} A", two_decimals(max)),
            Self::Opp => format!("0 – {} W", whole(max)),
            Self::Otp => format!("0 – {} °C", whole(max)),
        }
    }

    /// The label of the live column: `out`, `out`, `out`, `temp`, `in`.
    pub fn live_label(self) -> &'static str {
        match self {
            Self::Ovp | Self::Ocp | Self::Opp => "out",
            Self::Otp => "temp",
            Self::Lvp => "in",
        }
    }

    /// The live value shown against the limit, in thousandths.
    pub fn live(self, dump: &Dump) -> u32 {
        let milli = |x: f32| (f64::from(x.max(0.0)) * 1000.0).round() as u32;
        match self {
            Self::Ovp => milli(dump.output.voltage),
            Self::Ocp => milli(dump.output.current),
            Self::Opp => milli(dump.output.power),
            Self::Otp => milli(dump.temperature),
            Self::Lvp => milli(dump.input_voltage),
        }
    }

    /// Load as a fraction of the limit (§8.3 table).
    pub fn load(self, dump: &Dump, limit: u32) -> f32 {
        if limit == 0 {
            return 0.0;
        }
        match self {
            // LVP approaches 100 % as the input sags toward the limit.
            Self::Lvp => {
                let vin = self.live(dump);
                if vin == 0 {
                    0.0
                } else {
                    limit as f32 / vin as f32
                }
            }
            _ => self.live(dump) as f32 / limit as f32,
        }
    }

    /// Live value text for the table.
    pub fn live_text(self, dump: &Dump) -> String {
        let live = self.live(dump);
        match self {
            Self::Ovp | Self::Lvp => format!("{} V", two_decimals(live)),
            Self::Ocp => format!("{} A", two_decimals(live)),
            Self::Opp => format!("{} W", two_decimals(live)),
            Self::Otp => format!("{} °C", one_decimal(live)),
        }
    }
}

/// What the validation line says about a candidate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Check {
    pub ok: bool,
    /// After the `✓ ` or `✗ `.
    pub text: String,
    /// The shorter form of the Compact edit box.
    pub compact: String,
}

/// The values the rules compare a candidate with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Context {
    pub set_voltage: u32,
    pub set_current: u32,
    /// Live input voltage in mV.
    pub vin: u32,
    /// Temperature in m°C.
    pub temperature: u32,
}

/// One step below `limit` unless the limit sits at its device maximum, where
/// equality is allowed (LIM-04).
fn cap(value: u32, max: u32, margin: u32) -> u32 {
    if value >= max {
        value
    } else {
        value.saturating_sub(margin)
    }
}

/// Is `candidate` a valid new value for `limit` (SAFETY.md §1)?
pub fn check(limit: Limit, candidate: u32, dump: &Dump, ctx: &Context) -> Check {
    let max = limit.range_max(dump);
    if candidate > max {
        return Check {
            ok: false,
            text: "above device range".to_string(),
            compact: format!("above range {}", limit.range_text(max)),
        };
    }
    // (valid, good text, bad text, compact relation without "not ")
    let (ok, good, bad, relation) = match limit {
        Limit::Ovp => (
            ctx.set_voltage <= cap(candidate, max, LIMIT_MARGIN_MV),
            "OVP stays above it",
            "OVP not above it".to_string(),
            format!("above set {} V", two_decimals(ctx.set_voltage)),
        ),
        Limit::Ocp => {
            let set = format!("{}.{:03} A", ctx.set_current / 1000, ctx.set_current % 1000);
            (
                ctx.set_current <= cap(candidate, max, LIMIT_MARGIN_MA),
                "OCP stays above it",
                "OCP not above it".to_string(),
                format!("above limit {set}"),
            )
        }
        Limit::Opp => {
            let power = u64::from(ctx.set_voltage) * u64::from(ctx.set_current);
            let watts = one_decimal((power / 1000) as u32);
            (
                power <= u64::from(cap(candidate, max, LIMIT_MARGIN_MW)) * 1000,
                "OPP stays above it",
                "OPP not above it".to_string(),
                format!("above power {watts} W"),
            )
        }
        Limit::Lvp => (
            candidate + LIMIT_MARGIN_MV <= ctx.vin,
            "LVP stays below it",
            "LVP not below it".to_string(),
            format!("below input {} V", two_decimals(ctx.vin)),
        ),
        // Below the temperature the unit trips at once, even with the output
        // off (PROTOCOL.md §7.9).
        Limit::Otp => (
            candidate >= ctx.temperature.saturating_add(100),
            "OTP stays above it",
            "OTP not above it".to_string(),
            format!("above temp {} °C", one_decimal(ctx.temperature)),
        ),
    };
    Check {
        ok,
        text: if ok { good.to_string() } else { bad },
        compact: if ok {
            relation
        } else {
            format!("not {relation}")
        },
    }
}

/// The related setpoint of the edit box: label and value (§8.3).
pub fn related(limit: Limit, ctx: &Context) -> (&'static str, String) {
    match limit {
        Limit::Ovp => (
            "Voltage set",
            format!("{} V", two_decimals(ctx.set_voltage)),
        ),
        Limit::Ocp => (
            "Current limit",
            format!("{}.{:03} A", ctx.set_current / 1000, ctx.set_current % 1000),
        ),
        Limit::Opp => {
            let power = u64::from(ctx.set_voltage) * u64::from(ctx.set_current) / 1000;
            ("Set power", format!("{} W", one_decimal(power as u32)))
        }
        Limit::Lvp => ("Input now", format!("{} V", two_decimals(ctx.vin))),
        Limit::Otp => ("Temp now", format!("{} °C", one_decimal(ctx.temperature))),
    }
}

/// The third info line: what is drawn now and the margin to `value`.
/// Returns label, drawn text and the margin text (§8.3).
pub fn drawn(
    limit: Limit,
    dump: &Dump,
    value: u32,
    ctx: &Context,
) -> (&'static str, String, String) {
    let live = limit.live(dump);
    let gap = value.saturating_sub(live);
    match limit {
        Limit::Ovp => (
            "Drawn now",
            format!("{}.{:03} V", live / 1000, live % 1000),
            format!("margin {} V", two_decimals(gap)),
        ),
        Limit::Ocp => (
            "Drawn now",
            format!("{}.{:03} A", live / 1000, live % 1000),
            format!("margin {} A", two_decimals(gap)),
        ),
        Limit::Opp => (
            "Drawn now",
            format!("{} W", two_decimals(live)),
            format!("margin {} W", one_decimal(gap)),
        ),
        Limit::Otp => (
            "Margin",
            format!("{} °C", one_decimal(value.saturating_sub(ctx.temperature))),
            String::new(),
        ),
        Limit::Lvp => (
            "Margin",
            format!("{} V", two_decimals(ctx.vin.saturating_sub(value))),
            String::new(),
        ),
    }
}

/// An existing violation of an ordering rule, as the Protection screen shows
/// it in place of "no protection tripped" (§8.3). The app never edits the
/// limit itself (LIM-06).
pub fn violation(dump: &Dump, ctx: &Context) -> Option<(Limit, String)> {
    for limit in ALL {
        let value = limit.value(dump);
        let result = check(limit, value, dump, ctx);
        if !result.ok && value <= limit.range_max(dump) {
            let text = match limit {
                Limit::Ovp => format!(
                    "Set voltage {} V is not below OVP {} V",
                    two_decimals(ctx.set_voltage),
                    two_decimals(value)
                ),
                Limit::Ocp => format!(
                    "Set current {}.{:03} A is not below OCP {} A",
                    ctx.set_current / 1000,
                    ctx.set_current % 1000,
                    two_decimals(value)
                ),
                Limit::Opp => format!("Set power is not below OPP {} W", one_decimal(value)),
                Limit::Lvp => format!(
                    "Input {} V is not above LVP {} V",
                    two_decimals(ctx.vin),
                    two_decimals(value)
                ),
                Limit::Otp => format!(
                    "Temperature {} °C is not below OTP {} °C",
                    one_decimal(ctx.temperature),
                    one_decimal(value)
                ),
            };
            return Some((limit, text));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use protocol::{Mode, OutputReading, Preset};

    pub(crate) fn dump() -> Dump {
        let preset = Preset {
            voltage: Millivolts::new(5000),
            current: Milliamps::new(1000),
        };
        Dump {
            input_voltage: 20.25,
            set_voltage: Millivolts::new(5000),
            set_current: Milliamps::new(1500),
            output: OutputReading {
                voltage: 4.998,
                current: 0.512,
                power: 2.56,
            },
            temperature: 24.3,
            presets: [preset; 6],
            ovp: Millivolts::new(30_000),
            ocp: Milliamps::new(5_100),
            opp: Milliwatts::new(150_000),
            otp: MilliCelsius::new(80_000),
            lvp: Millivolts::new(5_000),
            brightness: 3,
            volume: 1,
            flag_d8: 1,
            capacity_ah: 0.012,
            energy_wh: 0.06,
            output_on: true,
            protection: 0,
            mode: Mode::ConstantVoltage,
            flag_e1: 1,
            max_voltage: Millivolts::new(20_050),
            max_current: Milliamps::new(5_100),
            ovp_range: Millivolts::new(30_000),
            ocp_range: Milliamps::new(5_100),
            opp_range: Milliwatts::new(150_000),
            otp_range: MilliCelsius::new(99_000),
            lvp_range: Millivolts::new(30_000),
        }
    }

    fn ctx() -> Context {
        Context {
            set_voltage: 5_000,
            set_current: 1_500,
            vin: 20_250,
            temperature: 24_300,
        }
    }

    #[test]
    fn pre_digit_formats_keep_their_leading_zeros() {
        assert_eq!(Limit::Ovp.digits(30_000), "30.00");
        assert_eq!(Limit::Ocp.digits(5_100), "05.10");
        assert_eq!(Limit::Opp.digits(150_000), "150.0");
        assert_eq!(Limit::Opp.digits(7_500), "007.5");
        assert_eq!(Limit::Otp.digits(80_000), "80.0");
        assert_eq!(Limit::Lvp.digits(5_000), "05.00");
    }

    #[test]
    fn pre_table_texts_follow_section_4_3() {
        assert_eq!(Limit::Ovp.text(30_000), "30.00 V");
        assert_eq!(Limit::Ocp.text(5_100), "5.10 A");
        assert_eq!(Limit::Opp.text(150_000), "150.0 W");
        assert_eq!(Limit::Otp.text(80_000), "80.0 °C");
        assert_eq!(Limit::Lvp.text(5_000), "5.00 V");
        assert_eq!(Limit::Ovp.range_text(30_000), "0 – 30 V");
        assert_eq!(Limit::Ocp.range_text(5_100), "0 – 5.10 A");
        assert_eq!(Limit::Opp.range_text(150_000), "0 – 150 W");
        assert_eq!(Limit::Otp.range_text(99_000), "0 – 99 °C");
    }

    #[test]
    fn pre_cursor_digits_map_to_characters_and_weights_agree() {
        for limit in ALL {
            let text = limit.digits(limit.default_value());
            assert_eq!(
                limit.weights().len(),
                text.chars().filter(|c| *c != '.').count()
            );
            for digit in 0..limit.weights().len() {
                let ch = text.chars().nth(limit.char_of_digit(digit)).unwrap();
                assert!(ch.is_ascii_digit(), "{limit:?} digit {digit}");
            }
        }
        assert_eq!(Limit::Ocp.initial_digit(), 1, "05.10: the 5");
        assert_eq!(
            Limit::Opp.initial_digit(),
            2,
            "150.0: the 0 before the point"
        );
    }

    #[test]
    fn lim_04_a_limit_must_stay_above_its_setpoint() {
        let d = dump();
        // OVP: set V 5.00 V; 5.01 V is one step above, 5.00 V is not.
        assert!(check(Limit::Ovp, 5_010, &d, &ctx()).ok);
        let bad = check(Limit::Ovp, 5_000, &d, &ctx());
        assert!(!bad.ok);
        assert_eq!(bad.text, "OVP not above it");
        assert_eq!(bad.compact, "not above set 5.00 V");
        // OCP: set I 1.500 A.
        assert!(check(Limit::Ocp, 1_501, &d, &ctx()).ok);
        assert!(!check(Limit::Ocp, 1_500, &d, &ctx()).ok);
        assert_eq!(
            check(Limit::Ocp, 5_100, &d, &ctx()).text,
            "OCP stays above it"
        );
        // OPP: 5 V x 1.5 A = 7.5 W.
        assert!(check(Limit::Opp, 7_600, &d, &ctx()).ok);
        assert!(!check(Limit::Opp, 7_500, &d, &ctx()).ok);
        // LVP stays below the input.
        assert!(check(Limit::Lvp, 20_240, &d, &ctx()).ok);
        assert!(!check(Limit::Lvp, 20_250, &d, &ctx()).ok);
        // OTP above the temperature, or the unit trips at once.
        assert!(check(Limit::Otp, 24_400, &d, &ctx()).ok);
        assert!(!check(Limit::Otp, 20_000, &d, &ctx()).ok);
    }

    #[test]
    fn lim_04_a_limit_at_its_device_maximum_allows_equality() {
        let mut d = dump();
        d.set_voltage = Millivolts::new(30_000);
        let mut c = ctx();
        c.set_voltage = 30_000;
        assert!(
            check(Limit::Ovp, 30_000, &d, &c).ok,
            "OVP 30.00 V with 30.00 V set"
        );
    }

    #[test]
    fn lim_03_a_candidate_above_the_device_range_is_invalid() {
        let d = dump();
        let result = check(Limit::Ocp, 5_110, &d, &ctx());
        assert!(!result.ok);
        assert_eq!(result.text, "above device range");
        assert_eq!(result.compact, "above range 0 – 5.10 A");
    }

    #[test]
    fn lim_06_an_existing_violation_is_reported_and_not_fixed() {
        let mut d = dump();
        d.ovp = Millivolts::new(5_000);
        let (limit, text) = violation(&d, &ctx()).expect("a violation");
        assert_eq!(limit, Limit::Ovp);
        assert_eq!(text, "Set voltage 5.00 V is not below OVP 5.00 V");
        assert_eq!(violation(&dump(), &ctx()), None);
    }

    #[test]
    fn pre_load_fractions_match_the_table() {
        let d = dump();
        let pct = |limit: Limit| (limit.load(&d, limit.value(&d)) * 100.0).round() as u32;
        assert_eq!(pct(Limit::Ovp), 17);
        assert_eq!(pct(Limit::Ocp), 10);
        assert_eq!(pct(Limit::Opp), 2);
        assert_eq!(pct(Limit::Otp), 30);
        assert_eq!(pct(Limit::Lvp), 25, "LVP over the input voltage");
    }

    #[test]
    fn pre_defaults_are_the_ones_observed_on_a_fresh_unit() {
        let values: Vec<u32> = ALL.iter().map(|l| l.default_value()).collect();
        assert_eq!(values, [30_000, 5_100, 150_000, 80_000, 5_000]);
        assert_eq!(Limit::Ocp.write(5_100).thousandths(), 5_100);
    }
}
