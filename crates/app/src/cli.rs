//! The command line (CLI-01): `--port <path>`, `--help`, `--version`, and
//! nothing else. Usage errors exit with status 2 (APP-06).

use clap::{Arg, ArgAction, Command};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Cli {
    pub port: Option<String>,
}

fn command() -> Command {
    Command::new("dps150")
        .about("Terminal interface for the FNIRSI DPS-150 bench power supply")
        .version(env!("CARGO_PKG_VERSION"))
        .disable_help_flag(true)
        .disable_version_flag(true)
        .arg(
            Arg::new("port")
                .long("port")
                .value_name("path")
                .help("Serial port of the supply; skips auto-detection"),
        )
        .arg(
            Arg::new("help")
                .long("help")
                .action(ArgAction::Help)
                .help("Print help"),
        )
        .arg(
            Arg::new("version")
                .long("version")
                .action(ArgAction::Version)
                .help("Print version"),
        )
}

/// Parses `args` (without the program name handling: `args[0]` is it). A usage
/// error, `--help` or `--version` comes back as the clap error, whose
/// `exit_code` is 2 for usage and 0 for help and version.
pub fn parse_args<I, T>(args: I) -> Result<Cli, clap::Error>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let matches = command().try_get_matches_from(args)?;
    Ok(Cli {
        port: matches.get_one::<String>("port").cloned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_01_accepts_port_help_and_version_only() {
        assert_eq!(parse_args(["dps150"]).unwrap(), Cli { port: None });
        let cli = parse_args(["dps150", "--port", "/dev/ttyACM0"]).unwrap();
        assert_eq!(cli.port.as_deref(), Some("/dev/ttyACM0"));
        for args in [
            vec!["dps150", "-h"],
            vec!["dps150", "-V"],
            vec!["dps150", "-p", "x"],
            vec!["dps150", "--baud", "9600"],
            vec!["dps150", "extra"],
            vec!["dps150", "--port"],
        ] {
            let error = parse_args(args.clone()).unwrap_err();
            assert_eq!(error.exit_code(), 2, "{args:?}");
        }
    }

    #[test]
    fn cli_01_help_and_version_are_not_errors() {
        for flag in ["--help", "--version"] {
            let error = parse_args(["dps150", flag]).unwrap_err();
            assert_eq!(error.exit_code(), 0, "{flag}");
        }
    }
}
