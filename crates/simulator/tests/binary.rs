//! Level 2 (DEVOPS.md §4): the real `dps150-sim` binary on a pty, opened by
//! path with `serialport` as the app will.
#![cfg(target_os = "linux")]

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use protocol::{Dir, Frame, Reader, Register};

struct Sim {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    port: Box<dyn serialport::SerialPort>,
    reader: Reader,
    start: Instant,
}

impl Sim {
    fn start() -> Self {
        let mut child = Command::new(env!("CARGO_BIN_EXE_dps150-sim"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("start dps150-sim");
        let stdin = child.stdin.take().unwrap();
        let mut stdout = BufReader::new(child.stdout.take().unwrap());
        let path = listening_path(&mut stdout);
        let port = serialport::new(&path, 115_200)
            .timeout(Duration::from_millis(50))
            .open()
            .expect("open the pty by path");
        Self {
            child,
            stdin,
            stdout,
            port,
            reader: Reader::new(Dir::DeviceToHost),
            start: Instant::now(),
        }
    }

    /// One control command; returns the lines before `ok` or the error line.
    fn command(&mut self, line: &str) -> Vec<String> {
        writeln!(self.stdin, "{line}").unwrap();
        let mut lines = Vec::new();
        loop {
            let mut text = String::new();
            self.stdout.read_line(&mut text).expect("a reply line");
            let text = text.trim_end().to_string();
            if text == "ok" || text.starts_with("error:") {
                lines.push(text);
                return lines;
            }
            lines.push(text);
        }
    }

    /// Frames read within `wait`.
    fn frames(&mut self, wait: Duration) -> Vec<Frame> {
        let end = Instant::now() + wait;
        let mut frames = Vec::new();
        let mut buffer = [0u8; 256];
        while Instant::now() < end {
            if let Ok(count) = self.port.read(&mut buffer) {
                frames.extend(self.reader.feed(self.start.elapsed(), &buffer[..count]));
            }
        }
        frames
    }

    fn send(&mut self, frame: &Frame) {
        self.port.write_all(&frame.encode()).unwrap();
    }
}

impl Drop for Sim {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn listening_path(stdout: &mut BufReader<ChildStdout>) -> String {
    let mut first = String::new();
    stdout.read_line(&mut first).expect("first line");
    first
        .trim_end()
        .strip_prefix("dps150-sim: listening on ")
        .unwrap_or_else(|| panic!("unexpected first line: {first:?}"))
        .to_string()
}

#[test]
fn sim_03_serves_the_protocol_on_a_pty() {
    let mut sim = Sim::start();
    assert!(sim.frames(Duration::from_millis(400)).is_empty(), "silent");

    sim.send(&Frame::connect());
    let frames = sim.frames(Duration::from_millis(700));
    let ids: Vec<u8> = frames.iter().map(Frame::id).collect();
    assert!(
        ids.starts_with(&[0xC3, 0xC0, 0xE2, 0xE3, 0xC4]),
        "{ids:02X?}"
    );

    sim.send(&Frame::get(Register::Model));
    let frames = sim.frames(Duration::from_millis(300));
    let model = frames.iter().find(|frame| frame.id() == 0xDE).unwrap();
    assert_eq!(model.data(), b"DPS-150");
}

#[test]
fn sim_11_commands_print_ok_or_an_error_and_state_lists_the_device() {
    let mut sim = Sim::start();
    // Blank lines and comments get no reply, so the next reply is for `load`.
    writeln!(sim.stdin, "\n# a comment").unwrap();
    assert_eq!(sim.command("load open"), ["ok"]);
    assert_eq!(sim.command("dance"), ["error: unknown command: dance"]);
    let state = sim.command("state");
    assert_eq!(state.last().map(String::as_str), Some("ok"));
    assert!(state.contains(&"load open".to_string()), "{state:?}");
    assert!(state.contains(&"output off".to_string()), "{state:?}");
}

#[test]
fn sim_11_faults_reach_the_app_over_the_pty() {
    let mut sim = Sim::start();
    sim.send(&Frame::connect());
    assert!(!sim.frames(Duration::from_millis(400)).is_empty());

    assert_eq!(sim.command("unplug"), ["ok"]);
    sim.frames(Duration::from_millis(50));
    assert!(sim.frames(Duration::from_millis(500)).is_empty(), "silent");

    assert_eq!(sim.command("plug"), ["ok"]);
    sim.send(&Frame::connect());
    assert!(!sim.frames(Duration::from_millis(400)).is_empty());

    assert_eq!(sim.command("corrupt 1000"), ["ok"]);
    sim.frames(Duration::from_millis(50));
    sim.frames(Duration::from_millis(400));
    assert!(sim.reader.stats().bad_checksum > 0);
}

#[test]
fn sim_11_close_and_reopen_give_a_new_pty() {
    let mut sim = Sim::start();
    assert_eq!(sim.command("close"), ["ok"]);
    writeln!(sim.stdin, "reopen").unwrap();
    let path = listening_path(&mut sim.stdout);
    let mut line = String::new();
    sim.stdout.read_line(&mut line).unwrap();
    assert_eq!(line.trim_end(), "ok");
    let mut port = serialport::new(&path, 115_200)
        .timeout(Duration::from_millis(50))
        .open()
        .expect("open the new pty");
    port.write_all(&Frame::connect().encode()).unwrap();
    let mut seen = Vec::new();
    let mut buffer = [0u8; 64];
    let end = Instant::now() + Duration::from_millis(600);
    while Instant::now() < end {
        if let Ok(count) = port.read(&mut buffer) {
            seen.extend_from_slice(&buffer[..count]);
        }
    }
    assert!(!seen.is_empty(), "the new pty is served");
}

#[test]
fn sim_11_quit_exits() {
    let mut sim = Sim::start();
    assert_eq!(sim.command("quit"), ["ok"]);
    let status = sim.child.wait().unwrap();
    assert!(status.success());
}
