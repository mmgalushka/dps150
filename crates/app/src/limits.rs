//! Ranges, ordering rules and knob steps (SAFETY.md §1, LIM-01 to LIM-09).
//!
//! Pure integer arithmetic on thousandths: voltages in mV, currents in mA,
//! power in mW. A product of volts and amps is `mV × mA` µW.

use protocol::Dump;

use crate::constants::{LIMIT_MARGIN_MA, LIMIT_MARGIN_MV, LIMIT_MARGIN_MW};

/// Which setpoint an operation is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Quantity {
    Voltage,
    Current,
}

/// The limit that stopped a value (LIM-08).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Limit {
    LiveMax(u32),
    Ovp(u32),
    Ocp(u32),
    Opp(u32),
}

/// Why a typed value is refused, with the numbers its message needs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reject {
    AboveLiveMax {
        quantity: Quantity,
        max: u32,
    },
    NotBelowOvp {
        ovp: u32,
    },
    NotBelowOcp {
        ocp: u32,
    },
    /// `power` is the product the value would give, in mW.
    NotBelowOpp {
        power: u32,
        opp: u32,
    },
}

/// One of the two texts of a message (DESIGN.md §10).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Text {
    pub standard: String,
    pub compact: String,
}

/// What a knob step did.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Step {
    pub value: u32,
    pub notice: Option<StepNotice>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepNotice {
    AtMinimum(Quantity),
    Stopped(Limit),
}

/// Everything the rules need, taken from one dump and the values on screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bounds {
    pub set_v: u32,
    pub set_i: u32,
    pub live_max_v: u32,
    pub live_max_i: u32,
    pub device_max_v: u32,
    pub ovp: u32,
    pub ovp_max: u32,
    pub ocp: u32,
    pub ocp_max: u32,
    pub opp: u32,
    pub opp_max: u32,
}

impl Bounds {
    /// `set_v` and `set_i` are the setpoints as shown (they may be ahead of
    /// the dump while a write is pending).
    pub fn new(dump: &Dump, set_v: u32, set_i: u32) -> Self {
        Self {
            set_v,
            set_i,
            live_max_v: dump.max_voltage.get(),
            live_max_i: dump.max_current.get(),
            device_max_v: dump.ovp_range.get(),
            ovp: dump.ovp.get(),
            ovp_max: dump.ovp_range.get(),
            ocp: dump.ocp.get(),
            ocp_max: dump.ocp_range.get(),
            opp: dump.opp.get(),
            opp_max: dump.opp_range.get(),
        }
    }

    /// Highest voltage the live maximum and the device range allow.
    fn live_voltage(&self) -> u32 {
        self.live_max_v.min(self.device_max_v)
    }

    /// Highest voltage below OVP; a limit at its device maximum allows
    /// equality (LIM-04).
    fn ovp_cap(&self) -> u32 {
        if self.ovp >= self.ovp_max {
            self.ovp
        } else {
            self.ovp.saturating_sub(LIMIT_MARGIN_MV)
        }
    }

    fn ocp_cap(&self) -> u32 {
        if self.ocp >= self.ocp_max {
            self.ocp
        } else {
            self.ocp.saturating_sub(LIMIT_MARGIN_MA)
        }
    }

    /// Highest power below OPP, in µW.
    fn opp_cap_micro(&self) -> u64 {
        let milli = if self.opp >= self.opp_max {
            self.opp
        } else {
            self.opp.saturating_sub(LIMIT_MARGIN_MW)
        };
        u64::from(milli) * 1000
    }

    /// Highest voltage at the current setpoint and the limit that sets it.
    pub fn voltage_ceiling(&self) -> (u32, Limit) {
        let mut best = (self.live_voltage(), Limit::LiveMax(self.live_voltage()));
        let ovp = self.ovp_cap();
        if ovp < best.0 {
            best = (ovp, Limit::Ovp(self.ovp));
        }
        if self.set_i > 0 {
            let opp =
                (self.opp_cap_micro() / u64::from(self.set_i)).min(u64::from(u32::MAX)) as u32;
            if opp < best.0 {
                best = (opp, Limit::Opp(self.opp));
            }
        }
        best
    }

    /// Highest current at the voltage setpoint and the limit that sets it.
    pub fn current_ceiling(&self) -> (u32, Limit) {
        let mut best = (self.live_max_i, Limit::LiveMax(self.live_max_i));
        let ocp = self.ocp_cap();
        if ocp < best.0 {
            best = (ocp, Limit::Ocp(self.ocp));
        }
        if self.set_v > 0 {
            let opp =
                (self.opp_cap_micro() / u64::from(self.set_v)).min(u64::from(u32::MAX)) as u32;
            if opp < best.0 {
                best = (opp, Limit::Opp(self.opp));
            }
        }
        best
    }

    /// May the voltage setpoint become `value`? Typed entry (LIM-08).
    pub fn check_voltage(&self, value: u32) -> Result<(), Reject> {
        if value > self.live_voltage() {
            return Err(Reject::AboveLiveMax {
                quantity: Quantity::Voltage,
                max: self.live_voltage(),
            });
        }
        if value > self.ovp_cap() {
            return Err(Reject::NotBelowOvp { ovp: self.ovp });
        }
        let power = u64::from(value) * u64::from(self.set_i);
        if power > self.opp_cap_micro() {
            return Err(Reject::NotBelowOpp {
                power: (power / 1000) as u32,
                opp: self.opp,
            });
        }
        Ok(())
    }

    pub fn check_current(&self, value: u32) -> Result<(), Reject> {
        if value > self.live_max_i {
            return Err(Reject::AboveLiveMax {
                quantity: Quantity::Current,
                max: self.live_max_i,
            });
        }
        if value > self.ocp_cap() {
            return Err(Reject::NotBelowOcp { ocp: self.ocp });
        }
        let power = u64::from(self.set_v) * u64::from(value);
        if power > self.opp_cap_micro() {
            return Err(Reject::NotBelowOpp {
                power: (power / 1000) as u32,
                opp: self.opp,
            });
        }
        Ok(())
    }

    /// A knob step by `weight` (§7.1). It never rejects: it stops at the
    /// limit and says which. A step never moves a value that is already past
    /// its ceiling further up.
    pub fn step(&self, quantity: Quantity, weight: u32, up: bool) -> Step {
        let (current, ceiling) = match quantity {
            Quantity::Voltage => (self.set_v, self.voltage_ceiling()),
            Quantity::Current => (self.set_i, self.current_ceiling()),
        };
        if up {
            let wanted = current.saturating_add(weight);
            if wanted <= ceiling.0 {
                return Step {
                    value: wanted,
                    notice: None,
                };
            }
            Step {
                value: ceiling.0.max(current),
                notice: Some(StepNotice::Stopped(ceiling.1)),
            }
        } else if weight >= current {
            // Reaching zero is fine; being at zero already is the notice.
            Step {
                value: 0,
                notice: (current == 0 || weight > current)
                    .then_some(StepNotice::AtMinimum(quantity)),
            }
        } else {
            Step {
                value: current - weight,
                notice: None,
            }
        }
    }
}

/// Digit weights in thousandths, index 0 is the leftmost digit (§7.1).
pub fn digit_weight(quantity: Quantity, digit: usize) -> u32 {
    match quantity {
        Quantity::Voltage => [10_000, 1_000, 100, 10][digit.min(3)],
        Quantity::Current => [1_000, 100, 10, 1][digit.min(3)],
    }
}

// ---- parsing typed text (LIM-09) ---------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Parsed {
    /// Nothing typed: the entry is cancelled.
    Empty,
    Value(u32),
    /// Not a decimal number, such as a lone `.`.
    NotANumber,
}

/// Parses typed decimal text to thousandths, rounded half up to 10 mV for
/// voltage and to 1 mA for current.
pub fn parse_typed(text: &str, quantity: Quantity) -> Parsed {
    parse_decimal(
        text,
        match quantity {
            Quantity::Voltage => 10,
            Quantity::Current => 1,
        },
    )
}

/// Parses typed decimal text to thousandths, rounded half up to a multiple
/// of `step` thousandths.
pub fn parse_decimal(text: &str, step: u32) -> Parsed {
    if text.is_empty() {
        return Parsed::Empty;
    }
    if text.chars().filter(|c| *c == '.').count() > 1
        || !text.chars().all(|c| c.is_ascii_digit() || c == '.')
        || !text.chars().any(|c| c.is_ascii_digit())
    {
        return Parsed::NotANumber;
    }
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    let whole: u32 = if whole.is_empty() {
        0
    } else {
        whole.parse().unwrap_or(0)
    };
    // Three fractional digits, padded; a fourth would round (not reachable
    // with five characters, but handled).
    let mut digits = fraction.chars().map(|c| c.to_digit(10).unwrap_or(0));
    let mut thousandths = whole.saturating_mul(1000);
    for place in [100, 10, 1] {
        thousandths = thousandths.saturating_add(digits.next().unwrap_or(0) * place);
    }
    if digits.next().is_some_and(|digit| digit >= 5) {
        thousandths = thousandths.saturating_add(1);
    }
    let step = step.max(1);
    Parsed::Value(thousandths.saturating_add(step / 2) / step * step)
}

// ---- texts (DESIGN.md §10) -----------------------------------------------------

/// `20.05` from 20050.
pub fn two_decimals(thousandths: u32) -> String {
    let hundredths = (u64::from(thousandths) + 5) / 10;
    format!("{}.{:02}", hundredths / 100, hundredths % 100)
}

/// `150.0` from 150000.
pub fn one_decimal(thousandths: u32) -> String {
    let tenths = (u64::from(thousandths) + 50) / 100;
    format!("{}.{}", tenths / 10, tenths % 10)
}

impl Reject {
    /// The message for the typed text `typed` (placeholders of §10).
    pub fn text(self, typed: &str) -> Text {
        match self {
            Self::AboveLiveMax { quantity, max } => {
                let (unit, max) = (unit(quantity), two_decimals(max));
                Text {
                    standard: format!("{typed} {unit} is above the live maximum {max} {unit}"),
                    compact: format!("{typed} {unit} > {max} {unit} live max"),
                }
            }
            Self::NotBelowOvp { ovp } => {
                let ovp = two_decimals(ovp);
                Text {
                    standard: format!(
                        "{typed} V is not below OVP {ovp} V. Change OVP on the Protection screen (o)"
                    ),
                    compact: format!("{typed} V: not below OVP {ovp} V"),
                }
            }
            Self::NotBelowOcp { ocp } => {
                let ocp = two_decimals(ocp);
                Text {
                    standard: format!(
                        "{typed} A is not below OCP {ocp} A. Change OCP on the Protection screen (o)"
                    ),
                    compact: format!("{typed} A: not below OCP {ocp} A"),
                }
            }
            // Not in the §10 table: written in its style, 72 cells at most.
            Self::NotBelowOpp { power, opp } => {
                let (power, opp) = (one_decimal(power), one_decimal(opp));
                Text {
                    standard: format!(
                        "{power} W is not below OPP {opp} W. Change OPP on the Protection screen (o)"
                    ),
                    compact: format!("{power} W: not below OPP {opp} W"),
                }
            }
        }
    }
}

fn unit(quantity: Quantity) -> &'static str {
    match quantity {
        Quantity::Voltage => "V",
        Quantity::Current => "A",
    }
}

impl StepNotice {
    /// The notice of §10; the Compact text is the same.
    pub fn text(self) -> String {
        match self {
            Self::AtMinimum(quantity) => format!("At minimum 0.00 {}", unit(quantity)),
            Self::Stopped(Limit::LiveMax(max)) => {
                // The unit follows from the magnitude of the quantity at hand
                // being unknown here, so the caller supplies it (see model).
                format!("At live maximum {}", two_decimals(max))
            }
            Self::Stopped(Limit::Ovp(x)) => format!("Limited by OVP {} V", two_decimals(x)),
            Self::Stopped(Limit::Ocp(x)) => format!("Limited by OCP {} A", two_decimals(x)),
            Self::Stopped(Limit::Opp(x)) => format!("Limited by OPP {} W", one_decimal(x)),
        }
    }

    /// Same text with the unit of a live-maximum stop filled in.
    pub fn text_for(self, quantity: Quantity) -> String {
        match self {
            Self::Stopped(Limit::LiveMax(_)) => format!("{} {}", self.text(), unit(quantity)),
            other => other.text(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The example reading of DESIGN.md §0.2 plus the default limits.
    fn bounds(set_v: u32, set_i: u32) -> Bounds {
        Bounds {
            set_v,
            set_i,
            live_max_v: 20_050,
            live_max_i: 5_100,
            device_max_v: 30_000,
            ovp: 30_000,
            ovp_max: 30_000,
            ocp: 5_100,
            ocp_max: 5_100,
            opp: 150_000,
            opp_max: 150_000,
        }
    }

    #[test]
    fn lim_02_set_voltage_stops_at_the_live_maximum() {
        let b = bounds(20_000, 1_000);
        assert_eq!(b.check_voltage(20_050), Ok(()));
        assert_eq!(
            b.check_voltage(20_060),
            Err(Reject::AboveLiveMax {
                quantity: Quantity::Voltage,
                max: 20_050
            })
        );
        // The device range caps it when the input could give more.
        let mut b = bounds(5_000, 1_000);
        b.live_max_v = 32_000;
        assert_eq!(b.voltage_ceiling().0, 30_000);
    }

    #[test]
    fn lim_02_set_current_stops_at_the_live_maximum() {
        let b = bounds(5_000, 1_000);
        assert_eq!(b.check_current(5_100), Ok(()));
        assert!(matches!(
            b.check_current(5_101),
            Err(Reject::AboveLiveMax {
                quantity: Quantity::Current,
                max: 5_100
            })
        ));
    }

    #[test]
    fn lim_04_a_setpoint_stays_one_step_below_a_limit_that_is_not_at_its_maximum() {
        let mut b = bounds(5_000, 1_000);
        b.ovp = 13_000;
        assert_eq!(b.check_voltage(12_990), Ok(()));
        assert_eq!(
            b.check_voltage(13_000),
            Err(Reject::NotBelowOvp { ovp: 13_000 })
        );
        assert_eq!(b.voltage_ceiling(), (12_990, Limit::Ovp(13_000)));
        let mut b = bounds(5_000, 1_000);
        b.ocp = 1_200;
        assert_eq!(b.check_current(1_199), Ok(()));
        assert_eq!(
            b.check_current(1_200),
            Err(Reject::NotBelowOcp { ocp: 1_200 })
        );
    }

    #[test]
    fn lim_04_a_limit_at_its_device_maximum_allows_equality() {
        let mut b = bounds(5_000, 1_000);
        b.live_max_v = 30_000;
        assert_eq!(b.check_voltage(30_000), Ok(()), "OVP at 30.00 V");
        let b = bounds(5_000, 1_000);
        assert_eq!(b.check_current(5_100), Ok(()), "OCP at 5.10 A");
        // 30 V x 5 A = 150 W is allowed with OPP at its maximum.
        let mut b = bounds(30_000, 5_000);
        b.live_max_v = 30_000;
        assert_eq!(b.check_current(5_000), Ok(()));
        assert!(b.check_current(5_001).is_err(), "150.03 W is over");
    }

    #[test]
    fn lim_04_power_must_stay_below_opp() {
        let mut b = bounds(10_000, 1_000);
        b.opp = 20_000;
        // One step (0.1 W) below 20.0 W is 19.9 W: 10 V x 1.99 A is the most.
        assert_eq!(b.check_current(1_990), Ok(()));
        assert_eq!(
            b.check_current(1_991),
            Err(Reject::NotBelowOpp {
                power: 19_910,
                opp: 20_000
            })
        );
        assert_eq!(b.current_ceiling(), (1_990, Limit::Opp(20_000)));
        // The voltage side uses the current setpoint.
        let mut b = bounds(5_000, 2_000);
        b.opp = 20_000;
        assert_eq!(b.check_voltage(9_950), Ok(()));
        assert!(matches!(
            b.check_voltage(9_960),
            Err(Reject::NotBelowOpp { .. })
        ));
        // No current, no power.
        let mut b = bounds(5_000, 0);
        b.opp = 100;
        assert_eq!(b.check_voltage(20_050), Ok(()));
    }

    #[test]
    fn lim_05_the_live_maximum_is_named_before_a_protection_limit() {
        let mut b = bounds(5_000, 1_000);
        b.ovp = 13_000;
        assert_eq!(
            b.check_voltage(35_000),
            Err(Reject::AboveLiveMax {
                quantity: Quantity::Voltage,
                max: 20_050
            })
        );
        // Equal ceilings: the live maximum is reported, not the limit.
        let mut b2 = bounds(5_000, 1_000);
        b2.ovp = 20_060;
        assert_eq!(b2.voltage_ceiling(), (20_050, Limit::LiveMax(20_050)));
    }

    #[test]
    fn lim_08_a_step_up_stops_at_the_ceiling_and_names_it() {
        let b = bounds(20_000, 1_000);
        assert_eq!(
            b.step(Quantity::Voltage, 10, true),
            Step {
                value: 20_010,
                notice: None
            }
        );
        let b = bounds(20_045, 1_000);
        let step = b.step(Quantity::Voltage, 10, true);
        assert_eq!(step.value, 20_050);
        assert_eq!(
            step.notice,
            Some(StepNotice::Stopped(Limit::LiveMax(20_050)))
        );
        assert_eq!(
            step.notice.unwrap().text_for(Quantity::Voltage),
            "At live maximum 20.05 V"
        );
        let mut b = bounds(12_980, 1_000);
        b.ovp = 13_000;
        let step = b.step(Quantity::Voltage, 100, true);
        assert_eq!(step.value, 12_990);
        assert_eq!(step.notice.unwrap().text(), "Limited by OVP 13.00 V");
    }

    #[test]
    fn lim_08_a_step_never_pushes_a_value_further_past_its_ceiling() {
        // The device already holds more than OVP allows (set on the panel).
        let mut b = bounds(14_000, 1_000);
        b.ovp = 13_000;
        let step = b.step(Quantity::Voltage, 10, true);
        assert_eq!(step.value, 14_000, "no change");
        assert!(step.notice.is_some());
        // Down is always allowed.
        assert_eq!(b.step(Quantity::Voltage, 1_000, false).value, 13_000);
    }

    #[test]
    fn lim_08_a_step_down_stops_at_zero_and_says_so_only_when_it_cannot_move() {
        let b = bounds(50, 1_000);
        let step = b.step(Quantity::Voltage, 100, false);
        assert_eq!(step.value, 0);
        assert_eq!(step.notice, Some(StepNotice::AtMinimum(Quantity::Voltage)));
        let b = bounds(0, 1_000);
        assert_eq!(b.step(Quantity::Voltage, 10, false).value, 0);
        assert_eq!(
            b.step(Quantity::Voltage, 10, false).notice.unwrap().text(),
            "At minimum 0.00 V"
        );
        let b = bounds(100, 1_000);
        let step = b.step(Quantity::Voltage, 100, false);
        assert_eq!(
            (step.value, step.notice),
            (0, None),
            "landing on zero is fine"
        );
    }

    #[test]
    fn edt_03_digit_weights_match_the_cursor_positions() {
        let v: Vec<u32> = (0..4).map(|d| digit_weight(Quantity::Voltage, d)).collect();
        let i: Vec<u32> = (0..4).map(|d| digit_weight(Quantity::Current, d)).collect();
        assert_eq!(v, [10_000, 1_000, 100, 10]);
        assert_eq!(i, [1_000, 100, 10, 1]);
    }

    #[test]
    fn lim_09_typed_text_is_rounded_to_the_resolution_before_the_check() {
        use Parsed::*;
        let v = Quantity::Voltage;
        assert_eq!(parse_typed("", v), Empty);
        assert_eq!(parse_typed("5", v), Value(5_000));
        assert_eq!(parse_typed("5.25", v), Value(5_250));
        assert_eq!(parse_typed("5.255", v), Value(5_260), "half up to 10 mV");
        assert_eq!(parse_typed("5.254", v), Value(5_250));
        assert_eq!(parse_typed(".5", v), Value(500));
        assert_eq!(parse_typed("5.", v), Value(5_000));
        assert_eq!(parse_typed("35", v), Value(35_000));
        assert_eq!(parse_typed(".", v), NotANumber);
        assert_eq!(parse_typed("1.2.3", v), NotANumber);
        assert_eq!(parse_typed("-1", v), NotANumber);
        assert_eq!(parse_typed("1e3", v), NotANumber);
        let a = Quantity::Current;
        assert_eq!(parse_typed("1.234", a), Value(1_234));
        assert_eq!(parse_typed("0.5", a), Value(500));
        assert_eq!(parse_typed("99999", a), Value(99_999_000));
    }

    #[test]
    fn lim_08_messages_follow_the_copy_table() {
        let live = Reject::AboveLiveMax {
            quantity: Quantity::Voltage,
            max: 20_050,
        };
        let text = live.text("35");
        assert_eq!(text.standard, "35 V is above the live maximum 20.05 V");
        assert_eq!(text.compact, "35 V > 20.05 V live max");
        let amps = Reject::AboveLiveMax {
            quantity: Quantity::Current,
            max: 5_100,
        }
        .text("35");
        assert_eq!(amps.standard, "35 A is above the live maximum 5.10 A");
        assert_eq!(amps.compact, "35 A > 5.10 A live max");
        let ovp = Reject::NotBelowOvp { ovp: 13_000 }.text("35");
        assert_eq!(
            ovp.standard,
            "35 V is not below OVP 13.00 V. Change OVP on the Protection screen (o)"
        );
        assert_eq!(ovp.compact, "35 V: not below OVP 13.00 V");
        let ocp = Reject::NotBelowOcp { ocp: 1_200 }.text("35");
        assert_eq!(ocp.compact, "35 A: not below OCP 1.20 A");
    }

    #[test]
    fn ui_09_every_reject_message_fits_its_row() {
        // 76 cells for Standard, 43 for Compact, for the longest values.
        let rejects = [
            Reject::AboveLiveMax {
                quantity: Quantity::Voltage,
                max: 30_000,
            },
            Reject::AboveLiveMax {
                quantity: Quantity::Current,
                max: 5_100,
            },
            Reject::NotBelowOvp { ovp: 29_990 },
            Reject::NotBelowOcp { ocp: 5_090 },
            Reject::NotBelowOpp {
                power: 149_900,
                opp: 150_000,
            },
        ];
        for reject in rejects {
            let text = reject.text("99999");
            assert!(text.standard.chars().count() <= 76, "{}", text.standard);
            assert!(text.compact.chars().count() <= 43, "{}", text.compact);
        }
    }

    #[test]
    fn lim_01_the_limit_check_cannot_be_fooled_by_huge_numbers() {
        let b = bounds(30_000, 5_100);
        assert!(b.check_voltage(u32::MAX).is_err());
        assert!(b.check_current(u32::MAX).is_err());
        // No overflow, and a value already past its ceiling is not pushed on.
        let step = b.step(Quantity::Voltage, u32::MAX, true);
        assert_eq!(step.value, 30_000);
        assert_eq!(b.step(Quantity::Voltage, u32::MAX, false).value, 0);
    }
}
