//! Level 2 (DEVOPS.md §4): the device thread on the real `serialport` code
//! path, talking to `dps150-sim` on a pty. Faults come in through the
//! simulator's stdin. Linux only: macOS refuses a pty at 115200.
#![cfg(target_os = "linux")]

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use device::{Cause, Device, Event, LinkConfig, Message};

struct Sim {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    path: String,
}

/// `dps150-sim` sits next to the test binary's `deps` directory. It is built
/// with the rest of the workspace (`cargo test --workspace`).
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
            .unwrap_or_else(|error| {
                panic!(
                    "start {}: {error}; run `cargo build -p simulator`",
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

fn started(sim: &Sim) -> Device {
    Device::start(&sim.path, LinkConfig::default()).expect("start against the simulator")
}

/// Waits for an event the test is looking for, ignoring model messages.
fn wait_for(device: &Device, limit: Duration, wanted: impl Fn(&Event) -> bool) -> Duration {
    let begin = Instant::now();
    while begin.elapsed() < limit {
        if let Ok(Message::Event(event)) = device.messages().recv_timeout(Duration::from_millis(20))
            && wanted(&event)
        {
            return begin.elapsed();
        }
    }
    panic!("event not seen within {limit:?}");
}

fn no_event(device: &Device, span: Duration) {
    let begin = Instant::now();
    while begin.elapsed() < span {
        if let Ok(Message::Event(event)) = device.messages().recv_timeout(Duration::from_millis(20))
        {
            panic!("unexpected event {event:?}");
        }
    }
}

fn is_lost(event: &Event) -> bool {
    matches!(event, Event::Lost(_))
}

fn is_restored(event: &Event) -> bool {
    matches!(event, Event::Online { restored: true })
}

#[test]
fn app_08_start_connects_and_streams_the_model_from_its_own_thread() {
    let sim = Sim::start();
    let device = started(&sim);
    let begin = Instant::now();
    let mut snapshot = None;
    while begin.elapsed() < Duration::from_secs(2) && snapshot.is_none() {
        if let Ok(Message::Model(model)) = device.messages().recv_timeout(Duration::from_millis(50))
        {
            snapshot = Some(model);
        }
    }
    let snapshot = snapshot.expect("a model message");
    assert_eq!(snapshot.model.as_deref(), Some("DPS-150"));
    assert_eq!(snapshot.dump.set_voltage.get(), 3300);
}

#[test]
fn app_01_a_missing_port_fails_the_start_with_the_documented_message() {
    let error = Device::start("/nonexistent/ttyDPS", LinkConfig::default())
        .err()
        .expect("start must fail");
    assert!(matches!(error.cause, Cause::Open(_)));
    assert!(
        error
            .to_string()
            .starts_with("dps150: cannot open /nonexistent/ttyDPS: "),
        "{error}"
    );
}

#[test]
fn app_01_a_silent_device_fails_the_start_after_three_seconds() {
    let mut sim = Sim::start();
    sim.command("unplug");
    let begin = Instant::now();
    let error = Device::start(&sim.path, LinkConfig::default())
        .err()
        .expect("start must fail");
    let took = begin.elapsed();
    assert_eq!(error.cause, Cause::NoAnswer);
    assert_eq!(
        error.to_string(),
        format!("dps150: device did not answer on {}", sim.path)
    );
    assert!(
        took >= Duration::from_millis(2900) && took < Duration::from_secs(4),
        "{took:?}"
    );
}

#[test]
fn lnk_01_unplug_loses_the_link_and_plug_restores_it() {
    let mut sim = Sim::start();
    let device = started(&sim);
    sim.command("unplug");
    let took = wait_for(&device, Duration::from_secs(2), is_lost);
    assert!(took < Duration::from_millis(800), "{took:?}");
    sim.command("plug");
    wait_for(&device, Duration::from_secs(5), is_restored);
}

#[test]
fn lnk_01_a_closed_pty_is_an_io_error_and_loses_the_link_at_once() {
    let mut sim = Sim::start();
    let device = started(&sim);
    sim.command("close");
    let took = wait_for(&device, Duration::from_secs(2), |event| {
        matches!(event, Event::Lost(Cause::Io(_)))
    });
    assert!(took < Duration::from_millis(300), "{took:?}");
}

#[test]
fn lnk_06_the_link_comes_back_after_corrupted_frames_stop() {
    let mut sim = Sim::start();
    let device = started(&sim);
    sim.command("corrupt 100000");
    wait_for(&device, Duration::from_secs(2), is_lost);
    sim.command("corrupt 0");
    wait_for(&device, Duration::from_secs(5), is_restored);
}

#[test]
fn app_03_frames_arriving_one_byte_at_a_time_are_reassembled() {
    let mut sim = Sim::start();
    sim.command("split 2");
    let device = started(&sim);
    no_event(&device, Duration::from_secs(1));
    drop(device);
}

#[test]
fn lnk_01_a_slow_reply_is_tolerated_and_a_very_slow_one_loses_the_link() {
    let mut sim = Sim::start();
    let device = started(&sim);
    sim.command("delay 125");
    no_event(&device, Duration::from_secs(2));
    sim.command("delay 600");
    wait_for(&device, Duration::from_secs(3), is_lost);
}
