//! Port selection (CLI-02, MAIN.md §6).

use std::fmt;

use crate::config::Config;

/// USB vendor and product id of the DPS-150 (PROTOCOL.md §1).
pub const USB_ID: (u16, u16) = (0x2E3C, 0x5740);

/// A serial port as the enumeration reports it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortInfo {
    pub path: String,
    /// USB vendor and product id, when the port is a USB one.
    pub usb: Option<(u16, u16)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolveError {
    NoDevice,
    Several(Vec<String>),
    NoPort,
}

impl fmt::Display for ResolveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDevice => write!(f, "dps150: no device found; use --port <path>"),
            Self::Several(paths) => {
                write!(f, "dps150: several devices found, use --port <path>:")?;
                for path in paths {
                    write!(f, "\n  {path}")?;
                }
                Ok(())
            }
            Self::NoPort => write!(
                f,
                "dps150: no port set; use --port <path> or set port in config.toml"
            ),
        }
    }
}

impl std::error::Error for ResolveError {}

/// `--port` wins. Otherwise, with `auto_detect`, exactly one USB `2E3C:5740`
/// port is used; without it the config `port` is used (CLI-02).
pub fn resolve_port(
    flag: Option<&str>,
    config: &Config,
    ports: &[PortInfo],
) -> Result<String, ResolveError> {
    if let Some(path) = flag {
        return Ok(path.to_string());
    }
    if !config.auto_detect {
        return match config.port.as_str() {
            "" => Err(ResolveError::NoPort),
            path => Ok(path.to_string()),
        };
    }
    let mut matches: Vec<&PortInfo> = ports.iter().filter(|p| p.usb == Some(USB_ID)).collect();
    // macOS lists one device twice, as `/dev/cu.X` and `/dev/tty.X`; use `cu.X`.
    let twin = |path: &str| path.replacen("/dev/tty.", "/dev/cu.", 1);
    let all: Vec<String> = matches.iter().map(|p| p.path.clone()).collect();
    matches.retain(|p| !p.path.starts_with("/dev/tty.") || !all.contains(&twin(&p.path)));
    matches.sort_by(|a, b| a.path.cmp(&b.path));
    match matches.as_slice() {
        [] => Err(ResolveError::NoDevice),
        [one] => Ok(one.path.clone()),
        many => Err(ResolveError::Several(
            many.iter().map(|p| p.path.clone()).collect(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn port(path: &str, usb: Option<(u16, u16)>) -> PortInfo {
        PortInfo {
            path: path.into(),
            usb,
        }
    }

    #[test]
    fn cli_02_flag_wins_over_everything() {
        let config = Config::default();
        assert_eq!(
            resolve_port(Some("/dev/x"), &config, &[]),
            Ok("/dev/x".into())
        );
    }

    #[test]
    fn cli_02_auto_detect_needs_exactly_one_matching_usb_port() {
        let config = Config::default();
        let other = port("/dev/ttyUSB0", Some((0x0403, 0x6001)));
        let unit = port("/dev/cu.usbmodem1", Some(USB_ID));
        let serial = port("/dev/ttyS0", None);
        assert_eq!(
            resolve_port(
                None,
                &config,
                &[other.clone(), unit.clone(), serial.clone()]
            ),
            Ok("/dev/cu.usbmodem1".into())
        );
        assert_eq!(
            resolve_port(None, &config, &[other, serial]),
            Err(ResolveError::NoDevice)
        );
        let second = port("/dev/cu.usbmodem2", Some(USB_ID));
        let error = resolve_port(None, &config, &[second, unit]).unwrap_err();
        assert_eq!(
            error,
            ResolveError::Several(vec!["/dev/cu.usbmodem1".into(), "/dev/cu.usbmodem2".into()])
        );
        assert!(error.to_string().contains("/dev/cu.usbmodem2"));
    }

    #[test]
    fn cli_02_a_mac_device_listed_as_cu_and_tty_is_one_device() {
        let config = Config::default();
        let cu = port("/dev/cu.usbmodem1", Some(USB_ID));
        let tty = port("/dev/tty.usbmodem1", Some(USB_ID));
        assert_eq!(
            resolve_port(None, &config, &[tty.clone(), cu]),
            Ok("/dev/cu.usbmodem1".into())
        );
        // A lone `tty.X` has no twin to prefer, so it is kept.
        assert_eq!(
            resolve_port(None, &config, &[tty]),
            Ok("/dev/tty.usbmodem1".into())
        );
        // Two real units are still ambiguous.
        let ports = [
            port("/dev/cu.usbmodem1", Some(USB_ID)),
            port("/dev/tty.usbmodem1", Some(USB_ID)),
            port("/dev/cu.usbmodem2", Some(USB_ID)),
            port("/dev/tty.usbmodem2", Some(USB_ID)),
        ];
        assert_eq!(
            resolve_port(None, &config, &ports),
            Err(ResolveError::Several(vec![
                "/dev/cu.usbmodem1".into(),
                "/dev/cu.usbmodem2".into()
            ]))
        );
    }

    #[test]
    fn cli_02_without_auto_detect_the_port_comes_from_the_config() {
        let mut config = Config {
            auto_detect: false,
            ..Config::default()
        };
        let unit = port("/dev/cu.usbmodem1", Some(USB_ID));
        assert_eq!(
            resolve_port(None, &config, std::slice::from_ref(&unit)),
            Err(ResolveError::NoPort)
        );
        config.port = "/dev/ttyACM9".into();
        assert_eq!(
            resolve_port(None, &config, &[unit]),
            Ok("/dev/ttyACM9".into())
        );
    }
}
