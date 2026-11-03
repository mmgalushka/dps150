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

    /// The simulator's `state`, as `name value` pairs.
    fn state(&mut self) -> std::collections::HashMap<String, String> {
        writeln!(self.stdin, "state").unwrap();
        let mut pairs = std::collections::HashMap::new();
        loop {
            let mut line = String::new();
            self.stdout.read_line(&mut line).unwrap();
            let line = line.trim_end();
            if line == "ok" {
                return pairs;
            }
            let (name, value) = line.split_once(' ').unwrap_or((line, ""));
            pairs.insert(name.to_string(), value.to_string());
        }
    }

    /// Waits until `name` has `value` in the simulator.
    fn wait_state(&mut self, name: &str, value: &str) {
        let begin = Instant::now();
        while begin.elapsed() < Duration::from_secs(5) {
            if self.state().get(name).map(String::as_str) == Some(value) {
                return;
            }
            thread::sleep(Duration::from_millis(50));
        }
        panic!("{name} is {:?}, wanted {value}", self.state().get(name));
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
    // A frame arrives in several writes, so wait for each part of it.
    for text in [
        "connected",
        "DPS-150 · fw V1.2",
        "▸ VOLTAGE",
        "OUTPUT  OFF",
        "Space  output",
    ] {
        tui.wait_for(text);
    }
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
    tui.wait_for("▸ CURRENT (A)"); // the focus is kept
    tui.send(b"q");
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn lnk_02_loss_shows_dashes_and_unknown_output_and_restore_brings_the_numbers_back() {
    let mut sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    sim.command("unplug");
    for text in [
        "no link",
        "Output state unknown, may still be ON",
        "OUTPUT  ?",
        "? OVP",
        "╺━╸", // measured digits are dashes
    ] {
        tui.wait_for(text);
    }
    let screen = tui.contents();
    assert!(
        !screen.contains("2.56 W") && !screen.contains("0.00 W"),
        "no stale power"
    );
    sim.command("plug");
    tui.wait_for("connected");
    tui.gone("no link");
    tui.wait_for("OUTPUT  OFF");
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
    tui.wait_for("need 44×16, have 43×15");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
}

// ---- editing --------------------------------------------------------------------

const UP: &[u8] = b"\x1b[A";
const DOWN: &[u8] = b"\x1b[B";
const RIGHT: &[u8] = b"\x1b[C";

#[test]
fn prt_05_starting_and_looking_changes_nothing_on_the_device() {
    let mut sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    thread::sleep(Duration::from_secs(1));
    let state = sim.state();
    assert_eq!(state["vset"], "3.3");
    assert_eq!(state["iset"], "2");
    assert_eq!(state["dropped"], "0");
    tui.send(b"q");
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn edt_03_the_arrow_keys_step_the_digit_under_the_cursor_on_the_device() {
    let mut sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    tui.send(UP);
    sim.wait_state("vset", "4.3");
    tui.send(RIGHT);
    tui.send(DOWN);
    sim.wait_state("vset", "4.2");
    // Three quick steps: coalesced, the last value arrives, nothing is lost.
    tui.send(UP);
    tui.send(UP);
    tui.send(UP);
    sim.wait_state("vset", "4.5");
    assert_eq!(sim.state()["dropped"], "0", "the write gap kept every SET");
    tui.send(b"q");
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn lim_09_typed_entry_sets_the_voltage_and_the_current() {
    let mut sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    tui.send(b"v");
    tui.wait_for("Typing a voltage. Range 0 – 20.03 V (live maximum)");
    tui.send(b"12.5\r");
    sim.wait_state("vset", "12.5");
    tui.send(b"i");
    tui.wait_for("Typing a current. Range 0 – 5.10 A");
    tui.send(b"0.75\r");
    sim.wait_state("iset", "0.75");
    tui.wait_for("▸ CURRENT");
    tui.send(b"q");
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn lim_08_a_value_above_the_live_maximum_is_rejected_and_nothing_is_sent() {
    let mut sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    tui.send(b"35\r");
    tui.wait_for("35 V is above the live maximum 20.03 V");
    thread::sleep(Duration::from_millis(500));
    let state = sim.state();
    assert_eq!(state["vset"], "3.3");
    assert_eq!(state["dropped"], "0");
    // The entry is still open and editable; Esc leaves it.
    tui.send(b"\x7f\x7f");
    tui.wait_for("Typing a voltage");
    tui.send(b"\x1b");
    // Esc alone is told from the start of a key sequence by a short pause.
    thread::sleep(Duration::from_millis(300));
    tui.send(b"q");
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn prt_03_a_value_the_device_keeps_is_shown_as_such() {
    let mut sim = Sim::start();
    sim.command("keep C1 4.0");
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    tui.send(UP);
    tui.wait_for("Device kept 4.00 V");
    tui.send(b"q");
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn lnk_03_with_the_link_down_edits_are_refused_and_nothing_goes_out_on_reconnect() {
    let mut sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    sim.command("unplug");
    tui.wait_for("no link");
    tui.send(UP);
    tui.wait_for("Link down. Changes are not sent.");
    sim.command("plug");
    tui.wait_for("connected");
    thread::sleep(Duration::from_millis(600));
    assert_eq!(sim.state()["vset"], "3.3");
    tui.send(b"q");
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn saf_03_while_the_output_is_on_a_change_is_held_back() {
    // The output is switched on from the simulator side (as the panel could);
    // this build has no way to confirm a live change.
    let mut sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    sim.command("panel on");
    tui.wait_for("OUTPUT  ON");
    tui.send(UP);
    tui.wait_for("Output is ON: changes are not sent yet");
    assert_eq!(sim.state()["vset"], "3.3");
    tui.send(b"q");
    assert_eq!(tui.wait_exit(), 0);
}
