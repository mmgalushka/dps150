//! Level 2 (DEVOPS.md §4) for the `dps150` binary: start-up failures and exit
//! codes, and the whole program on a pty against `dps150-sim`, drawn into a
//! terminal emulator. Linux only: macOS refuses a pty at 115200.
#![cfg(target_os = "linux")]

use std::fs::OpenOptions;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::process::CommandExt;
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
        // Make the test pty the app's controlling terminal. crossterm reads
        // the size and the keys from /dev/tty, so without this the app would
        // see whatever terminal `cargo test` itself is run from.
        // SAFETY: only async-signal-safe calls (setsid, ioctl) run between
        // fork and exec; fd 0 is the pty slave by then.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 || libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
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

    /// Resizes the terminal the way a window manager would. The emulator is
    /// resized first: once the pty has the new size the app may draw at once,
    /// and a frame written to an emulator of the old size wraps, and only
    /// the cells that change are ever sent again.
    fn resize(&mut self, rows: u16, cols: u16) {
        self.screen
            .lock()
            .unwrap()
            .screen_mut()
            .set_size(nz(rows), nz(cols));
        let path = self._slave.name().unwrap();
        Command::new("stty")
            .args([
                "-F",
                &path,
                "rows",
                &rows.to_string(),
                "cols",
                &cols.to_string(),
            ])
            .status()
            .unwrap();
        Command::new("kill")
            .args(["-WINCH", &self.child.id().to_string()])
            .status()
            .unwrap();
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
fn ui_11_the_mouse_is_captured_so_the_wheel_cannot_scroll_or_press_arrow_keys() {
    const MOUSE_ON: &str = "\x1b[?1000h";
    const MOUSE_OFF: &str = "\x1b[?1000l";
    let sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    assert!(tui.raw_text().contains(MOUSE_ON), "mouse captured");
    // Wheel up and down, and a click, as the terminal reports them (SGR).
    tui.send(b"\x1b[<64;10;5M\x1b[<65;10;5M\x1b[<0;10;5M\x1b[<0;10;5m");
    tui.wait_for("connected");
    tui.send(b"q");
    assert_eq!(tui.wait_exit(), 0);
    assert!(tui.raw_text().contains(MOUSE_OFF), "mouse released on exit");
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
    tui.resize(16, 50);
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

// ---- Presets and Protection ----------------------------------------------------------

const ENTER: &[u8] = b"\r";

fn config_dir(tag: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("dps150-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("dps150")).unwrap();
    for (name, text) in files {
        std::fs::write(dir.join("dps150").join(name), text).unwrap();
    }
    dir
}

#[test]
fn pre_01_the_presets_screen_lists_the_device_slots_and_esc_goes_back() {
    let sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    tui.send(b"p");
    for text in [
        "PRESETS",
        "╭─ Presets",
        "device M1–M6",
        "5.00 V  1.00 A",
        "(no preset matches)",
    ] {
        tui.wait_for(text);
    }
    tui.send(b"\x1b");
    tui.wait_for("MEASURED");
    tui.gone("PRESETS");
    tui.send(b"q");
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn pre_01_editing_a_slot_writes_both_values_to_the_device_and_nothing_else() {
    let mut sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    tui.send(b"p");
    tui.wait_for("PRESETS");
    tui.send(b"\x1b[B\x1b[B"); // M3
    tui.send(b"e");
    tui.wait_for("⏎  write");
    tui.send(UP); // voltage units: 5 V -> 6 V
    tui.send(b"\t");
    tui.send(UP); // current tenths: 1.0 A -> 1.1 A
    tui.send(ENTER);
    sim.wait_state("m3", "6/1.1");
    let state = sim.state();
    assert_eq!(state["m1"], "5/1");
    assert_eq!(state["vset"], "3.3", "the setpoints are untouched");
    assert_eq!(state["dropped"], "0");
    tui.wait_for("6.00 V  1.10 A");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn pre_01_recall_writes_the_setpoints_by_the_list_and_by_the_chord() {
    let mut sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    // Make M2 distinctive first: 6 V / 1 A via the list editor.
    tui.send(b"p");
    tui.send(b"\x1b[B");
    tui.send(b"e");
    tui.send(UP);
    tui.send(ENTER);
    sim.wait_state("m2", "6/1");
    // Recall it from the list.
    tui.send(ENTER);
    sim.wait_state("vset", "6");
    tui.wait_for("Recalled M2 · 6.00 V / 1.000 A");
    // And M1 from the main panel by the chord.
    tui.send(b"\x1b");
    thread::sleep(Duration::from_millis(300));
    tui.send(b"m");
    tui.wait_for("Recall which preset? 1-6, Esc cancels");
    tui.send(b"1");
    sim.wait_state("vset", "5");
    sim.wait_state("iset", "1");
    assert_eq!(sim.state()["dropped"], "0");
    tui.send(b"q");
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn saf_12_save_asks_first_and_a_no_changes_nothing() {
    let mut sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    tui.send(b"p");
    tui.send(b"s");
    tui.wait_for("Overwrite M1?  5.00 V/1.000 A → 3.30 V/2.000 A");
    tui.send(b"n");
    tui.wait_for("recall");
    thread::sleep(Duration::from_millis(400));
    assert_eq!(sim.state()["m1"], "5/1");
    tui.send(b"s");
    tui.wait_for("Overwrite M1?");
    tui.send(b"y");
    sim.wait_state("m1", "3.3/2");
    assert_eq!(sim.state()["dropped"], "0");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn pre_04_a_name_is_kept_in_presets_toml_and_comes_back_on_the_next_start() {
    let dir = config_dir("names", &[]);
    let xdg = dir.to_str().unwrap().to_string();
    let sim = Sim::start();
    let mut tui = Tui::start(&sim, &[("XDG_CONFIG_HOME", &xdg)]);
    tui.wait_for("connected");
    tui.send(b"p");
    tui.send(b"n");
    tui.wait_for("⏎  save");
    tui.send(b"bench psu");
    tui.send(ENTER);
    tui.wait_for("bench psu");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
    let text = std::fs::read_to_string(dir.join("dps150/presets.toml")).unwrap();
    assert!(text.contains("bench psu"), "{text}");
    assert!(!text.contains("5.0"), "voltages are never stored: {text}");

    let mut tui = Tui::start(&sim, &[("XDG_CONFIG_HOME", &xdg)]);
    tui.wait_for("connected");
    tui.send(b"p");
    tui.wait_for("bench psu");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn app_05_an_invalid_presets_file_is_reported_and_never_overwritten() {
    let broken = "names = 3\n";
    let dir = config_dir("badnames", &[("presets.toml", broken)]);
    let xdg = dir.to_str().unwrap().to_string();
    let sim = Sim::start();
    let mut tui = Tui::start(&sim, &[("XDG_CONFIG_HOME", &xdg)]);
    tui.wait_for("presets.toml: line 1: invalid value; using defaults");
    tui.send(b"p");
    tui.send(b"n");
    tui.send(b"x");
    tui.send(ENTER);
    tui.wait_for("Names not saved: presets.toml is invalid");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
    assert_eq!(
        std::fs::read_to_string(dir.join("dps150/presets.toml")).unwrap(),
        broken
    );
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn lim_05_a_protection_limit_is_edited_with_the_arrows_and_written_on_enter() {
    let mut sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    tui.send(b"o");
    for text in [
        "PROTECTION",
        "Protection limits",
        "no protection tripped",
        "Selected OVP",
    ] {
        tui.wait_for(text);
    }
    tui.send(ENTER);
    tui.wait_for("Editing OVP");
    tui.send(DOWN); // units digit: 30 V -> 29 V
    tui.send(ENTER);
    sim.wait_state("ovp", "29");
    tui.wait_for("Selected OVP");
    tui.wait_for("29.00 V");
    assert_eq!(sim.state()["dropped"], "0");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn lim_05_an_invalid_candidate_cannot_be_applied() {
    let mut sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    tui.send(b"o\x1b[B"); // OCP
    tui.send(ENTER);
    tui.wait_for("Editing OCP");
    // 5.10 A -> 2.10 A -> 1.10 A, below the 2.000 A current setpoint.
    tui.send(DOWN);
    tui.send(DOWN);
    tui.send(DOWN);
    tui.send(DOWN);
    tui.wait_for("✗ OCP not above it");
    tui.send(ENTER);
    thread::sleep(Duration::from_millis(500));
    assert_eq!(sim.state()["ocp"], "5.1", "enter is disabled while invalid");
    tui.wait_for("Editing OCP");
    tui.send(UP);
    tui.wait_for("✓ OCP stays above it");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn pro_typed_values_are_applied_r_loads_the_default_and_a_kept_value_is_shown() {
    let mut sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    tui.send(b"o");
    tui.wait_for("Selected OVP");
    // Typed: 1, 2 and Enter set OVP to 12 V.
    tui.send(b"12");
    tui.send(ENTER);
    sim.wait_state("ovp", "12");
    // From now on the device keeps 12 V whatever it is sent.
    sim.command("keep D1 12.0");
    tui.send(b"r");
    tui.wait_for("! default is the device maximum: no protection");
    tui.send(ENTER);
    tui.wait_for("Device kept 12.00 V");
    assert_eq!(sim.state()["ovp"], "12");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn lnk_03_with_the_link_down_the_secondary_screens_still_show_and_refuse_edits() {
    let mut sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    tui.send(b"p");
    tui.wait_for("PRESETS");
    sim.command("unplug");
    tui.wait_for("no link");
    tui.wait_for("-.--- V");
    tui.send(b"s");
    tui.wait_for("Link down. Changes are not sent.");
    sim.command("plug");
    tui.wait_for("connected");
    thread::sleep(Duration::from_millis(500));
    assert_eq!(sim.state()["m1"], "5/1");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
}

// ---- output control ----------------------------------------------------------------------

#[test]
fn saf_03_space_asks_then_y_turns_the_output_on_and_space_turns_it_off_without_asking() {
    let mut sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    tui.send(b" ");
    tui.wait_for("Turn output ON at 3.30 V / 2.000 A?");
    tui.wait_for("y  yes");
    assert_eq!(
        sim.state()["output"],
        "off",
        "nothing is sent while it asks"
    );
    // Enter does not answer it.
    tui.send(ENTER);
    thread::sleep(Duration::from_millis(300));
    assert_eq!(sim.state()["output"], "off");
    tui.send(b"y");
    sim.wait_state("output", "on");
    tui.wait_for("OUTPUT  ON");
    tui.send(b" ");
    sim.wait_state("output", "off");
    tui.wait_for("OUTPUT  OFF");
    assert_eq!(sim.state()["dropped"], "0");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn saf_05_n_declines_and_the_output_stays_off() {
    let mut sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    tui.send(b" ");
    tui.wait_for("Turn output ON");
    tui.send(b"n");
    tui.gone("Turn output ON");
    thread::sleep(Duration::from_millis(300));
    assert_eq!(sim.state()["output"], "off");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn saf_03_with_output_on_immediately_space_turns_it_on_at_once() {
    let dir = config_dir(
        "immediate",
        &[("config.toml", "output_on = \"immediate\"\n")],
    );
    let xdg = dir.to_str().unwrap().to_string();
    let mut sim = Sim::start();
    let mut tui = Tui::start(&sim, &[("XDG_CONFIG_HOME", &xdg)]);
    tui.wait_for("connected");
    tui.send(b" ");
    sim.wait_state("output", "on");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn saf_02_ctrl_x_turns_an_output_that_is_on_off_even_in_typed_entry() {
    let mut sim = Sim::start();
    sim.command("panel on");
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("OUTPUT  ON");
    tui.send(b"v");
    tui.wait_for("Typing a voltage");
    tui.send(&[0x18]); // Ctrl-X
    sim.wait_state("output", "off");
    tui.gone("Typing a voltage");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn saf_06_a_big_jump_with_the_output_on_asks_and_only_a_yes_sends_it() {
    let mut sim = Sim::start();
    sim.command("panel on");
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("OUTPUT  ON");
    tui.send(b"15\r");
    tui.wait_for("Set 15.00 V / 2.000 A (+11.70 V), output ON?");
    thread::sleep(Duration::from_millis(300));
    assert_eq!(sim.state()["vset"], "3.3", "held, nothing sent");
    // A second Enter, as after a double press, changes nothing.
    tui.send(ENTER);
    thread::sleep(Duration::from_millis(300));
    assert_eq!(sim.state()["vset"], "3.3");
    tui.send(b"y");
    sim.wait_state("vset", "15");
    tui.gone("output ON?");
    assert_eq!(sim.state()["dropped"], "0");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn saf_06_a_declined_jump_leaves_the_setpoint_and_space_cancels_it_and_switches_off() {
    let mut sim = Sim::start();
    sim.command("panel on");
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("OUTPUT  ON");
    tui.send(b"15\r");
    tui.wait_for("output ON?");
    tui.send(b"n");
    tui.gone("output ON?");
    assert_eq!(sim.state()["vset"], "3.3");
    tui.send(b"15\r");
    tui.wait_for("output ON?");
    tui.send(b" ");
    sim.wait_state("output", "off");
    assert_eq!(sim.state()["vset"], "3.3", "the jump was not applied");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn saf_08_a_recall_with_the_output_on_asks_first() {
    let mut sim = Sim::start();
    sim.command("panel on");
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("OUTPUT  ON");
    tui.send(b"m");
    tui.wait_for("Recall which preset?");
    tui.send(b"2");
    tui.wait_for("Recall M2 5.00 V / 1.000 A, output ON?");
    assert_eq!(sim.state()["vset"], "3.3");
    tui.send(b"y");
    sim.wait_state("vset", "5");
    sim.wait_state("iset", "1");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn pro_01_a_trip_is_shown_and_space_offers_to_clear_it_by_turning_on() {
    let mut sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    sim.command("trip otp");
    for text in [
        "PROTECTION TRIPPED",
        "OTP tripped",
        "output forced off",
        "● OTP",
    ] {
        tui.wait_for(text);
    }
    assert_eq!(sim.state()["protection"], "otp");
    tui.send(b" ");
    tui.wait_for("Clear OTP and turn output ON at 3.30 V / 2.000 A?");
    assert_eq!(sim.state()["protection"], "otp", "nothing sent yet");
    tui.send(b"y");
    sim.wait_state("output", "on");
    sim.wait_state("protection", "none");
    tui.gone("PROTECTION TRIPPED");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn saf_13_a_clear_that_trips_again_shows_the_trip_says_why_and_is_not_retried() {
    let mut sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    // The unit is hot: the trip is there, and so is its cause.
    sim.command("temp 85");
    tui.wait_for("OTP tripped");
    tui.send(b" ");
    tui.wait_for("Clear OTP (85.0 °C > 80.0 °C) and turn ON at 3.30 V / 2.000 A?");
    tui.send(b"y");
    // The clear turns the output on and it trips again at once.
    sim.wait_state("protection", "otp");
    sim.wait_state("output", "off");
    tui.wait_for("OTP tripped");
    thread::sleep(Duration::from_millis(700));
    assert_eq!(sim.state()["output"], "off", "no automatic retry");
    assert_eq!(sim.state()["dropped"], "0");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn lnk_04_an_off_queued_while_the_link_is_down_is_sent_first_after_the_reconnect() {
    let mut sim = Sim::start();
    sim.command("panel on");
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("OUTPUT  ON");
    sim.command("unplug");
    tui.wait_for("no link");
    tui.send(b" ");
    tui.wait_for("Off queued · use the front-panel button");
    assert_eq!(
        sim.state()["output"],
        "on",
        "the cable is out: nothing can be sent"
    );
    sim.command("plug");
    tui.wait_for("Link restored · queued OFF sent, output OFF");
    sim.wait_state("output", "off");
    tui.wait_for("OUTPUT  OFF");
    assert_eq!(sim.state()["dropped"], "0");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn lnk_04_a_second_space_while_down_clears_the_queue() {
    let mut sim = Sim::start();
    sim.command("panel on");
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("OUTPUT  ON");
    sim.command("unplug");
    tui.wait_for("no link");
    tui.send(b" ");
    tui.wait_for("Off queued");
    tui.send(b" ");
    tui.wait_for("Off queue cleared");
    sim.command("plug");
    tui.wait_for("Link restored · output ON at 3.30 V / 2.000 A");
    thread::sleep(Duration::from_millis(500));
    assert_eq!(sim.state()["output"], "on", "nothing was sent");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn saf_10_on_the_too_small_screen_space_only_switches_off_q_is_ignored_and_ctrl_c_quits() {
    let mut sim = Sim::start();
    sim.command("panel on");
    let mut tui = Tui::start_sized(&sim, &[], 15, 43);
    tui.wait_for("Terminal too small");
    tui.wait_for("output ON 3.30 V / 2.000 A");
    tui.send(b"q");
    thread::sleep(Duration::from_millis(300));
    assert!(tui.child.try_wait().unwrap().is_none(), "q is ignored");
    tui.send(b" ");
    sim.wait_state("output", "off");
    tui.wait_for("output OFF");
    tui.send(b" ");
    thread::sleep(Duration::from_millis(400));
    assert_eq!(sim.state()["output"], "off", "Space never turns it on here");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn saf_09_a_prompt_is_cancelled_when_the_terminal_becomes_too_small() {
    let sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    tui.send(b" ");
    tui.wait_for("Turn output ON");
    tui.resize(15, 43);
    tui.wait_for("Terminal too small");
    // Back to a usable size: the question did not come back.
    tui.resize(24, 80);
    tui.wait_for("MEASURED");
    assert!(!tui.contents().contains("Turn output ON"));
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
}

// ---- quitting and closing the session -------------------------------------------------------

/// What the program wrote after it left the alternate screen, escape
/// sequences removed: the last thing it wrote (SAF-11).
fn after_leaving(raw: &str) -> String {
    let tail = raw.rsplit(LEAVE_ALT).next().unwrap_or("");
    let mut text = String::new();
    let mut chars = tail.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // CSI: skip to the final byte.
            if chars.peek() == Some(&'[') {
                chars.next();
                for next in chars.by_ref() {
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            text.push(ch);
        }
    }
    text.trim().replace("\r\n", "\n")
}

fn kill(signal: &str, tui: &Tui) {
    Command::new("kill")
        .args([signal, &tui.child.id().to_string()])
        .status()
        .unwrap();
}

#[test]
fn saf_14_q_with_the_output_off_quits_at_once_and_closes_the_session() {
    let mut sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    assert_eq!(
        sim.state()["locked"],
        "true",
        "the connect frame locked the panel"
    );
    tui.send(b"q");
    assert_eq!(tui.wait_exit(), 0);
    assert_eq!(after_leaving(&tui.raw_text()), "", "nothing is printed");
    let state = sim.state();
    assert_eq!(
        state["locked"], "false",
        "the close frame released the panel"
    );
    assert_eq!(state["connected"], "false");
    assert_eq!(state["dropped"], "0", "the device kept the close frame");
}

#[test]
fn saf_11_q_with_the_output_on_asks_and_esc_stays() {
    let mut sim = Sim::start();
    sim.command("panel on");
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("OUTPUT  ON");
    tui.send(b"q");
    tui.wait_for("Quit with output ON (3.30 V)?");
    tui.wait_for("y  quit");
    tui.wait_for("o  off+quit");
    tui.send(b"\x1b");
    thread::sleep(Duration::from_millis(300));
    tui.gone("Quit with output ON");
    assert!(tui.child.try_wait().unwrap().is_none(), "still running");
    assert_eq!(sim.state()["locked"], "true");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn saf_11_y_quits_leaving_the_output_on_and_says_so_last() {
    let mut sim = Sim::start();
    sim.command("panel on");
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("OUTPUT  ON");
    tui.send(b"q");
    tui.wait_for("Quit with output ON");
    tui.send(b"y");
    assert_eq!(tui.wait_exit(), 0);
    assert_eq!(
        after_leaving(&tui.raw_text()),
        "dps150: output left ON (3.30 V / 2.000 A)"
    );
    let state = sim.state();
    assert_eq!(state["output"], "on", "left as it was");
    assert_eq!(state["locked"], "false", "but the panel is released");
}

#[test]
fn saf_11_o_turns_the_output_off_waits_for_the_read_back_and_then_quits() {
    let mut sim = Sim::start();
    sim.command("panel on");
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("OUTPUT  ON");
    tui.send(b"q");
    tui.wait_for("Quit with output ON");
    tui.send(b"o");
    assert_eq!(tui.wait_exit(), 0);
    assert_eq!(after_leaving(&tui.raw_text()), "dps150: output turned OFF");
    let state = sim.state();
    assert_eq!(state["output"], "off");
    assert_eq!(state["locked"], "false");
    assert_eq!(state["dropped"], "0");
}

#[test]
fn saf_11_with_on_exit_turn_off_q_turns_it_off_without_asking() {
    let dir = config_dir("onexit", &[("config.toml", "on_exit = \"off\"\n")]);
    let xdg = dir.to_str().unwrap().to_string();
    let mut sim = Sim::start();
    sim.command("panel on");
    let mut tui = Tui::start(&sim, &[("XDG_CONFIG_HOME", &xdg)]);
    tui.wait_for("OUTPUT  ON");
    tui.send(b"q");
    assert_eq!(tui.wait_exit(), 0);
    assert!(
        !tui.raw_text().contains("Quit with output ON"),
        "no question"
    );
    assert_eq!(after_leaving(&tui.raw_text()), "dps150: output turned OFF");
    assert_eq!(sim.state()["output"], "off");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn saf_11_ctrl_c_follows_the_setting_without_asking() {
    // Default: leave.
    let mut sim = Sim::start();
    sim.command("panel on");
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("OUTPUT  ON");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
    assert_eq!(
        after_leaving(&tui.raw_text()),
        "dps150: output left ON (3.30 V / 2.000 A)"
    );
    assert_eq!(sim.state()["output"], "on");
    // On exit = turn off.
    let dir = config_dir("ctrlc", &[("config.toml", "on_exit = \"off\"\n")]);
    let xdg = dir.to_str().unwrap().to_string();
    let mut sim = Sim::start();
    sim.command("panel on");
    let mut tui = Tui::start(&sim, &[("XDG_CONFIG_HOME", &xdg)]);
    tui.wait_for("OUTPUT  ON");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
    assert_eq!(after_leaving(&tui.raw_text()), "dps150: output turned OFF");
    assert_eq!(sim.state()["output"], "off");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn saf_11_sigterm_and_sighup_quit_without_a_prompt_and_close_the_session() {
    for signal in ["-TERM", "-HUP"] {
        let mut sim = Sim::start();
        sim.command("panel on");
        let mut tui = Tui::start(&sim, &[]);
        tui.wait_for("OUTPUT  ON");
        kill(signal, &tui);
        assert_eq!(tui.wait_exit(), 0, "{signal}");
        assert_eq!(
            after_leaving(&tui.raw_text()),
            "dps150: output left ON (3.30 V / 2.000 A)",
            "{signal}"
        );
        assert!(!tui.raw_text().contains("Quit with output ON"), "{signal}");
        assert_eq!(sim.state()["locked"], "false", "{signal}: released");
    }
}

#[test]
fn saf_11_sigterm_turns_the_output_off_when_the_setting_says_so() {
    let dir = config_dir("sigterm", &[("config.toml", "on_exit = \"off\"\n")]);
    let xdg = dir.to_str().unwrap().to_string();
    let mut sim = Sim::start();
    sim.command("panel on");
    let mut tui = Tui::start(&sim, &[("XDG_CONFIG_HOME", &xdg)]);
    tui.wait_for("OUTPUT  ON");
    kill("-TERM", &tui);
    assert_eq!(tui.wait_exit(), 0);
    assert_eq!(after_leaving(&tui.raw_text()), "dps150: output turned OFF");
    assert_eq!(sim.state()["output"], "off");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn saf_11_an_off_that_cannot_be_confirmed_exits_1_and_still_releases_the_panel() {
    let mut sim = Sim::start();
    sim.command("panel on");
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("OUTPUT  ON");
    sim.command("keep DB 1"); // the device keeps the output on
    tui.send(b"q");
    tui.wait_for("Quit with output ON");
    tui.send(b"o");
    assert_eq!(tui.wait_exit(), 1);
    assert_eq!(
        after_leaving(&tui.raw_text()),
        "dps150: could not confirm output OFF, check the unit"
    );
    let state = sim.state();
    assert_eq!(state["output"], "on");
    assert_eq!(state["locked"], "false", "so the panel button works");
}

#[test]
fn saf_11_with_the_link_down_q_asks_and_y_quits_saying_the_state_is_unknown() {
    let mut sim = Sim::start();
    sim.command("panel on");
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("OUTPUT  ON");
    sim.command("unplug");
    tui.wait_for("no link");
    tui.send(b"q");
    tui.wait_for("Link down: cannot turn output off. Quit anyway?");
    tui.send(b"\x1b");
    thread::sleep(Duration::from_millis(300));
    assert!(tui.child.try_wait().unwrap().is_none());
    tui.send(b"q");
    tui.wait_for("Quit anyway?");
    tui.send(b"y");
    assert_eq!(tui.wait_exit(), 0);
    assert_eq!(
        after_leaving(&tui.raw_text()),
        "dps150: link lost, output state unknown"
    );
    assert_eq!(sim.state()["output"], "on", "nothing could be sent");
}

#[test]
fn saf_14_the_last_write_is_sent_before_the_session_is_closed() {
    let mut sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    tui.send(UP);
    tui.send(b"q");
    assert_eq!(tui.wait_exit(), 0);
    let state = sim.state();
    assert_eq!(state["vset"], "4.3", "the write was not lost");
    assert_eq!(state["locked"], "false");
    assert_eq!(
        state["dropped"], "0",
        "the gap held between the write and the close"
    );
}

#[test]
fn saf_11_on_the_too_small_screen_only_ctrl_c_quits_and_it_closes_the_session() {
    let mut sim = Sim::start();
    let mut tui = Tui::start_sized(&sim, &[], 15, 43);
    tui.wait_for("Terminal too small");
    tui.send(b"q");
    thread::sleep(Duration::from_millis(300));
    assert!(tui.child.try_wait().unwrap().is_none(), "q is ignored");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
    assert_eq!(sim.state()["locked"], "false");
}

// ---- help ----------------------------------------------------------------------------------

#[test]
fn ui_08_question_mark_opens_the_help_and_esc_closes_it_on_a_real_terminal() {
    let sim = Sim::start();
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("connected");
    tui.send(b"?");
    for text in [
        "Help · main panel",
        "? or Esc closes",
        "Glossary",
        "Esc  close",
    ] {
        tui.wait_for(text);
    }
    assert!(!tui.contents().contains("MEASURED"), "it replaces the body");
    tui.send(b"\x1b");
    tui.wait_for("MEASURED");
    tui.gone("Help · main panel");
    tui.send(b"p?");
    tui.wait_for("Help · presets");
    tui.wait_for("PRESETS");
    tui.send(b"?");
    tui.gone("Help · presets");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn ui_08_the_compact_help_turns_pages_with_the_arrows() {
    let sim = Sim::start();
    let mut tui = Tui::start_sized(&sim, &[], 16, 44);
    tui.wait_for("connected");
    tui.send(b"?");
    tui.wait_for("Help · main panel 1/2");
    tui.send(DOWN);
    tui.wait_for("Help · main panel 2/2");
    tui.wait_for("Glossary");
    tui.send(UP);
    tui.wait_for("Help · main panel 1/2");
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
}

#[test]
fn saf_01_under_the_help_space_switches_an_output_that_is_on_off_and_never_on() {
    let mut sim = Sim::start();
    sim.command("panel on");
    let mut tui = Tui::start(&sim, &[]);
    tui.wait_for("OUTPUT  ON");
    tui.send(b"?");
    tui.wait_for("Help · main panel");
    tui.send(b" ");
    sim.wait_state("output", "off");
    tui.send(b" ");
    thread::sleep(Duration::from_millis(400));
    assert_eq!(sim.state()["output"], "off", "no ON from under the overlay");
    tui.send(b"\x1b");
    tui.wait_for("OUTPUT  OFF");
    assert!(
        !tui.contents().contains("Turn output ON"),
        "no hidden question"
    );
    tui.send(&[0x03]);
    assert_eq!(tui.wait_exit(), 0);
}
