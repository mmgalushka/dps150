//! Level 2 (DEVOPS.md §4) for the `dps150` binary: start-up failures and exit
//! codes, and the whole program on a pty against `dps150-sim`, drawn into a
//! terminal emulator. Linux only: macOS refuses a pty at 115200.
#![cfg(target_os = "linux")]

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serialport::{SerialPort, TTYPort};

fn nz(value: u16) -> std::num::NonZeroU16 {
    std::num::NonZeroU16::new(value).expect("non-zero size")
}

const APP: &str = env!("CARGO_BIN_EXE_dps150");

struct Sim {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    path: String,
}

fn sim_binary() -> PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    if path.ends_with("deps") {
        path.pop();
    }
    path.join("dps150-sim")
}

impl Sim {
    fn start() -> Self {
        let binary = sim_binary();
        let mut child = Command::new(&binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap_or_else(|e| {
                panic!(
                    "start {}: {e}; run `cargo build -p simulator`",
                    binary.display()
                )
            });
        let stdin = child.stdin.take().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let mut first = String::new();
        stdout.read_line(&mut first).unwrap();
        let path = first
            .trim_end()
            .strip_prefix("dps150-sim: listening on ")
            .unwrap_or_else(|| panic!("unexpected first line {first:?}"))
            .to_string();
        Self {
            child,
            stdin,
            stdout,
            path,
        }
    }

    fn command(&mut self, line: &str) {
        writeln!(self.stdin, "{line}").unwrap();
        let mut reply = String::new();
        self.stdout.read_line(&mut reply).unwrap();
        assert_eq!(reply.trim_end(), "ok", "{line}");
    }
}

impl Drop for Sim {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

struct Run {
    status: Option<i32>,
    stdout: String,
    stderr: String,
}

/// Runs the app without a terminal until it exits.
fn run(args: &[&str], envs: &[(&str, &str)]) -> Run {
    let mut command = Command::new(APP);
    command
        .args(args)
        .env_remove("LC_ALL")
        .env_remove("LC_CTYPE")
        .env("LANG", "en_US.UTF-8")
        .env("HOME", "/nonexistent")
        .env_remove("XDG_CONFIG_HOME")
        .stdin(Stdio::null());
    for (name, value) in envs {
        command.env(name, value);
    }
    let output = command.output().expect("run dps150");
    Run {
        status: output.status.code(),
        stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
    }
}

// ---- exit codes and start-up failures --------------------------------------

#[test]
fn app_06_usage_errors_exit_2_and_help_and_version_exit_0() {
    for args in [&["--bogus"][..], &["-h"], &["--port"], &["extra"]] {
        let result = run(args, &[]);
        assert_eq!(result.status, Some(2), "{args:?}: {}", result.stderr);
        assert!(result.stdout.is_empty(), "{args:?}");
    }
    let help = run(&["--help"], &[]);
    assert_eq!(help.status, Some(0));
    assert!(help.stdout.contains("--port"));
    let version = run(&["--version"], &[]);
    assert_eq!(version.status, Some(0));
    assert!(version.stdout.starts_with("dps150 "));
}

#[test]
fn ui_10_a_non_utf8_locale_exits_before_the_alternate_screen() {
    let result = run(&["--port", "/dev/null"], &[("LANG", "C")]);
    assert_eq!(result.status, Some(1));
    assert_eq!(result.stderr.trim_end(), "dps150 needs a UTF-8 terminal");
    assert!(
        result.stdout.is_empty(),
        "nothing on stdout, no escape codes"
    );
}

#[test]
fn app_01_a_port_that_cannot_be_opened_exits_1_with_the_reason() {
    let result = run(&["--port", "/nonexistent/ttyDPS"], &[]);
    assert_eq!(result.status, Some(1));
    assert!(
        result
            .stderr
            .starts_with("dps150: cannot open /nonexistent/ttyDPS: "),
        "{}",
        result.stderr
    );
    assert!(result.stdout.is_empty());
}

#[test]
fn cli_02_without_a_port_and_without_a_matching_device_it_says_so() {
    let result = run(&[], &[]);
    assert_eq!(result.status, Some(1));
    assert!(
        result.stderr.starts_with("dps150: no device found"),
        "{}",
        result.stderr
    );
}

#[test]
fn app_01_a_silent_device_exits_1_after_three_seconds() {
    let mut sim = Sim::start();
    sim.command("unplug");
    let begin = Instant::now();
    let result = run(&["--port", &sim.path], &[]);
    assert_eq!(result.status, Some(1));
    assert_eq!(
        result.stderr.trim_end(),
        format!("dps150: device did not answer on {}", sim.path)
    );
    assert!(begin.elapsed() >= Duration::from_millis(2900));
}

#[test]
fn app_05_a_broken_config_is_reported_on_the_first_screen_and_never_overwritten() {
    let dir = std::env::temp_dir().join(format!("dps150-config-{}", std::process::id()));
    let config_dir = dir.join("dps150");
    std::fs::create_dir_all(&config_dir).unwrap();
    let file = config_dir.join("config.toml");
    std::fs::write(&file, "reconnect = true\nrefresh_hz = \"fast\"\n").unwrap();
    let sim = Sim::start();
    let mut tui = Tui::start(&sim, &[("XDG_CONFIG_HOME", dir.to_str().unwrap())]);
    tui.wait_for("config.toml: line 2: invalid value; using defaults");
    tui.send(b"q");
    tui.wait_exit();
    assert_eq!(
        std::fs::read_to_string(&file).unwrap(),
        "reconnect = true\nrefresh_hz = \"fast\"\n"
    );
    let _ = std::fs::remove_dir_all(dir);
}

// ---- the program on a pty ----------------------------------------------------

struct Tui {
    child: Child,
    master: TTYPort,
    screen: Arc<Mutex<vt100::Parser>>,
    raw: Arc<Mutex<Vec<u8>>>,
    _slave: TTYPort,
}

impl Tui {
    fn start(sim: &Sim, envs: &[(&str, &str)]) -> Self {
        Self::start_sized(sim, envs, 24, 80)
    }

    fn start_sized(sim: &Sim, envs: &[(&str, &str)], rows: u16, cols: u16) -> Self {
        let (mut master, slave) = TTYPort::pair().expect("a pty for the app's terminal");
        master.set_timeout(Duration::from_millis(20)).unwrap();
        let path = slave.name().unwrap();
        let status = Command::new("stty")
            .args([
                "-F",
                &path,
                "rows",
                &rows.to_string(),
                "cols",
                &cols.to_string(),
            ])
            .status()
            .expect("stty");
        assert!(status.success());
        let tty = || {
            OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
                .expect("open the pty slave")
        };
        let mut command = Command::new(APP);
        command
            .args(["--port", &sim.path])
            .env_remove("LC_ALL")
            .env_remove("LC_CTYPE")
            .env("LANG", "en_US.UTF-8")
            .env("COLORTERM", "truecolor")
            .env("TERM", "xterm-256color")
            .env("HOME", "/nonexistent")
            .env_remove("XDG_CONFIG_HOME")
            .env_remove("NO_COLOR")
            .stdin(Stdio::from(tty()))
            .stdout(Stdio::from(tty()))
            .stderr(Stdio::from(tty()));
        for (name, value) in envs {
            command.env(name, value);
        }
        let child = command.spawn().expect("start dps150");

        let screen = Arc::new(Mutex::new(vt100::Parser::new(nz(rows), nz(cols), 0)));
        let raw = Arc::new(Mutex::new(Vec::new()));
        let (reader_screen, reader_raw) = (Arc::clone(&screen), Arc::clone(&raw));
        let mut reader = master.try_clone_native().expect("clone master");
        thread::spawn(move || {
            let mut buffer = [0u8; 4096];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => return,
                    Ok(count) => {
                        reader_screen.lock().unwrap().process(&buffer[..count]);
                        reader_raw
                            .lock()
                            .unwrap()
                            .extend_from_slice(&buffer[..count]);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::TimedOut => {}
                    Err(_) => return,
                }
            }
        });
        Self {
            child,
            master,
            screen,
            raw,
            _slave: slave,
        }
    }

    fn contents(&self) -> String {
        self.screen.lock().unwrap().screen().contents()
    }

    fn wait_for(&self, text: &str) {
        let begin = Instant::now();
        while begin.elapsed() < Duration::from_secs(8) {
            if self.contents().contains(text) {
                return;
            }
            thread::sleep(Duration::from_millis(25));
        }
        panic!("{text:?} not on screen:\n{}", self.contents());
    }

    fn gone(&self, text: &str) {
        let begin = Instant::now();
        while begin.elapsed() < Duration::from_secs(8) {
            if !self.contents().contains(text) {
                return;
            }
            thread::sleep(Duration::from_millis(25));
        }
        panic!("{text:?} still on screen:\n{}", self.contents());
    }

    fn send(&mut self, bytes: &[u8]) {
        self.master.write_all(bytes).unwrap();
    }

    fn wait_exit(&mut self) -> i32 {
        let begin = Instant::now();
        while begin.elapsed() < Duration::from_secs(5) {
            if let Some(status) = self.child.try_wait().unwrap() {
                thread::sleep(Duration::from_millis(100));
                return status.code().unwrap_or(-1);
            }
            thread::sleep(Duration::from_millis(25));
        }
        let _ = self.child.kill();
        panic!("dps150 did not exit:\n{}", self.contents());
    }

    fn raw_text(&self) -> String {
        String::from_utf8_lossy(&self.raw.lock().unwrap()).into_owned()
    }
}

impl Drop for Tui {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

const ENTER_ALT: &str = "\x1b[?1049h";
const LEAVE_ALT: &str = "\x1b[?1049l";
const HIDE_CURSOR: &str = "\x1b[?25l";
const SHOW_CURSOR: &str = "\x1b[?25h";

#[test]
fn prt_05_the_main_panel_shows_the_device_and_changes_no_setting() {
    let sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    let screen = tui.contents();
    assert!(screen.contains("DPS-150 · fw V1.2"), "{screen}");
    assert!(screen.contains("▸ VOLTAGE"));
    assert!(screen.contains("OUTPUT  OFF"));
    assert!(screen.contains("Space  output"));
    // Set values of the simulator's initial state: 3.30 V / 2.000 A.
    tui.send(b"q");
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn ui_11_the_alternate_screen_and_cursor_are_entered_and_restored_on_quit() {
    let sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    let raw = tui.raw_text();
    assert!(raw.contains(ENTER_ALT) && raw.contains(HIDE_CURSOR));
    tui.send(b"q");
    assert_eq!(tui.wait_exit(), 0);
    let raw = tui.raw_text();
    assert!(
        raw.contains(LEAVE_ALT) && raw.contains(SHOW_CURSOR),
        "restored on exit"
    );
}

#[test]
fn ui_11_ctrl_c_quits_and_restores_the_terminal() {
    let sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
    assert!(tui.raw_text().contains(LEAVE_ALT));
}

#[test]
fn ui_11_sigterm_restores_the_terminal() {
    let sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    let status = Command::new("kill")
        .args(["-TERM", &tui.child.id().to_string()])
        .status()
        .unwrap();
    assert!(status.success());
    assert_eq!(tui.wait_exit(), 0);
    let raw = tui.raw_text();
    assert!(raw.contains(LEAVE_ALT) && raw.contains(SHOW_CURSOR));
}

#[test]
fn ui_12_resize_reevaluates_the_tier_and_keeps_the_state() {
    let sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("╭─ Status");
    tui.send(b"\t");
    tui.wait_for("▸ CURRENT");
    // Shrink the terminal below the Fluid floor: the Compact layout appears.
    let path = tui._slave.name().unwrap();
    Command::new("stty")
        .args(["-F", &path, "rows", "16", "cols", "50"])
        .status()
        .unwrap();
    {
        let mut screen = tui.screen.lock().unwrap();
        screen.screen_mut().set_size(nz(16), nz(50));
    }
    let pid = tui.child.id().to_string();
    Command::new("kill")
        .args(["-WINCH", &pid])
        .status()
        .unwrap();
    tui.wait_for("CURRENT (A)");
    assert!(
        tui.contents().contains("▸ CURRENT (A)"),
        "focus kept: {}",
        tui.contents()
    );
    tui.send(b"q");
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn lnk_02_loss_shows_dashes_and_unknown_output_and_restore_brings_the_numbers_back() {
    let mut sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    sim.command("unplug");
    tui.wait_for("no link");
    let screen = tui.contents();
    assert!(
        screen.contains("Output state unknown, may still be ON"),
        "{screen}"
    );
    assert!(screen.contains("OUTPUT  ?"));
    assert!(screen.contains("? OVP"));
    assert!(screen.contains("╺━╸"), "measured digits are dashes");
    assert!(
        !screen.contains("2.56 W") && !screen.contains("0.00 W"),
        "no stale power"
    );
    sim.command("plug");
    tui.wait_for("connected");
    tui.gone("no link");
    assert!(tui.contents().contains("OUTPUT  OFF"));
    tui.send(b"q");
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn lnk_06_with_reconnect_off_r_retries_once() {
    let dir = std::env::temp_dir().join(format!("dps150-reconnect-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("dps150")).unwrap();
    std::fs::write(dir.join("dps150/config.toml"), "reconnect = false\n").unwrap();
    let mut sim = Sim::start();
    let mut tui = Tui::start(&sim, &[("XDG_CONFIG_HOME", dir.to_str().unwrap())]);
    tui.wait_for("connected");
    sim.command("unplug");
    tui.wait_for("reconnect off · r retry");
    sim.command("plug");
    thread::sleep(Duration::from_millis(2500));
    assert!(tui.contents().contains("no link"), "no automatic retry");
    tui.send(b"r");
    tui.wait_for("connected");
    tui.send(b"q");
    assert_eq!(tui.wait_exit(), 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn ui_01_a_terminal_below_the_floor_shows_the_too_small_screen() {
    let sim = Sim::start();
    let mut tui = Tui::start_sized(&sim, &[], 15, 43);
    tui.wait_for("Terminal too small");
    assert!(tui.contents().contains("need 44×16, have 43×15"));
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
}
