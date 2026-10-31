//! Read-only probe of a real DPS-150, for the hardware session H1 Part A
//! (spec/PLAN.md). It sends the connect frame and GET frames and nothing else:
//! the protocol crate has no way to build a SET.
//!
//! ```text
//! cargo run --release -p protocol --example hw_probe -- <port> [--secs 60] [--hz 10] [--passive-secs 10] [--out DIR]
//! ```
//!
//! 1. Connect, then read the model, hardware and firmware strings.
//! 2. Passive phase: no polling, count the pushed frames per type.
//! 3. Poll phase: `GET FF` at the given rate, with the poll rules of
//!    spec/APP.md §2 (a tick is skipped while the previous reply is
//!    outstanding, and a skipped tick counts as missed, LNK-01).
//!
//! `--out DIR` saves the first `FF` reply as `ff-dump.bin`, a test fixture.

use std::collections::BTreeMap;
use std::error::Error;
use std::io::{ErrorKind, Read, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use protocol::{Dir, Frame, Reader, Register, Reply};

const BAUD: u32 = 115_200;
const READ_TIMEOUT: Duration = Duration::from_millis(2);
/// LNK-01: the link is lost after this many consecutive missed ticks.
const MISSED_TICKS: u32 = 3;

struct Args {
    port: String,
    secs: u64,
    hz: u64,
    passive_secs: u64,
    out: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        port: String::new(),
        secs: 60,
        hz: 10,
        passive_secs: 10,
        out: None,
    };
    let mut it = std::env::args().skip(1);
    while let Some(arg) = it.next() {
        let mut number = |name: &str| -> Result<u64, String> {
            it.next()
                .and_then(|v| v.parse().ok())
                .filter(|v| *v > 0)
                .ok_or(format!("{name} needs a positive number"))
        };
        match arg.as_str() {
            "--secs" => args.secs = number("--secs")?,
            "--hz" => args.hz = number("--hz")?,
            "--passive-secs" => args.passive_secs = number("--passive-secs")?,
            "--out" => args.out = Some(it.next().ok_or("--out needs a directory")?.into()),
            _ if args.port.is_empty() && !arg.starts_with("--") => args.port = arg,
            _ => return Err(format!("unknown argument: {arg}")),
        }
    }
    if args.port.is_empty() {
        return Err(
            "usage: hw_probe <port> [--secs N] [--hz N] [--passive-secs N] [--out DIR]".into(),
        );
    }
    Ok(args)
}

struct Link {
    port: Box<dyn serialport::SerialPort>,
    reader: Reader,
    start: Instant,
}

impl Link {
    fn send(&mut self, frame: &Frame) -> Result<(), Box<dyn Error>> {
        self.port.write_all(&frame.encode())?;
        self.port.flush()?;
        Ok(())
    }

    /// One read of at most `READ_TIMEOUT`; the frames it completes, each with the time it was seen.
    fn pump(&mut self) -> Result<Vec<(Instant, Frame)>, Box<dyn Error>> {
        let mut buf = [0; 1024];
        match self.port.read(&mut buf) {
            Ok(n) => {
                let now = Instant::now();
                let frames = self.reader.feed(now - self.start, &buf[..n]);
                Ok(frames.into_iter().map(|f| (now, f)).collect())
            }
            Err(e) if e.kind() == ErrorKind::TimedOut => Ok(Vec::new()),
            Err(e) => Err(e.into()),
        }
    }

    /// Waits up to `limit` for the reply to `GET register`.
    fn get(
        &mut self,
        register: Register,
        limit: Duration,
    ) -> Result<Option<Frame>, Box<dyn Error>> {
        self.send(&Frame::get(register))?;
        let until = Instant::now() + limit;
        while Instant::now() < until {
            for (_, frame) in self.pump()? {
                if frame.register() == Some(register) {
                    return Ok(Some(frame));
                }
            }
        }
        Ok(None)
    }
}

fn millis(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn percentile(sorted: &[Duration], p: f64) -> Duration {
    sorted[((sorted.len() - 1) as f64 * p / 100.0).round() as usize]
}

fn text_of(frame: Option<Frame>) -> String {
    match frame.map(|f| Reply::from_frame(&f)) {
        Some(Ok(Reply::Model(s) | Reply::HardwareVersion(s) | Reply::FirmwareVersion(s))) => s,
        Some(other) => format!("{other:?}"),
        None => "no reply".into(),
    }
}

fn print_rates(title: &str, counts: &BTreeMap<u8, u64>, elapsed: Duration) {
    let secs = elapsed.as_secs_f64();
    let total: u64 = counts.values().sum();
    println!(
        "{title}: {total} frames in {secs:.1} s = {:.2} frames/s",
        total as f64 / secs
    );
    for (id, n) in counts {
        println!("    type {id:#04x}: {n:5} = {:.2}/s", *n as f64 / secs);
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    let args = parse_args()?;
    let port = serialport::new(&args.port, BAUD)
        .timeout(READ_TIMEOUT)
        .open()?;
    let mut link = Link {
        port,
        reader: Reader::new(Dir::DeviceToHost),
        start: Instant::now(),
    };
    println!(
        "hw_probe: {} at {BAUD} baud, read-only (connect frame and GET only)",
        args.port
    );

    link.send(&Frame::connect())?;
    let settle = Instant::now() + Duration::from_secs(1);
    while Instant::now() < settle {
        link.pump()?;
    }
    for (name, register) in [
        ("model", Register::Model),
        ("hardware", Register::HardwareVersion),
        ("firmware", Register::FirmwareVersion),
    ] {
        println!(
            "{name}: {}",
            text_of(link.get(register, Duration::from_secs(1))?)
        );
    }

    let first = link
        .get(Register::Dump, Duration::from_secs(2))?
        .ok_or("no reply to GET FF")?;
    if let Some(dir) = &args.out {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join("ff-dump.bin"), first.encode())?;
        println!("saved {}", dir.join("ff-dump.bin").display());
    }
    println!("unit state at start: {:#?}", Reply::from_frame(&first)?);

    // Passive phase: pushed frames only.
    let mut counts = BTreeMap::new();
    let begin = Instant::now();
    while begin.elapsed() < Duration::from_secs(args.passive_secs) {
        for (_, frame) in link.pump()? {
            *counts.entry(frame.id()).or_insert(0) += 1;
        }
    }
    print_rates("passive (no polling)", &counts, begin.elapsed());

    // Poll phase.
    let period = Duration::from_nanos(1_000_000_000 / args.hz);
    let mut pushed = BTreeMap::new();
    let mut latencies = Vec::new();
    let mut reply_times: Vec<Instant> = Vec::new();
    let (mut sent, mut missed, mut run, mut longest_run) = (0_u64, 0_u64, 0_u32, 0_u32);
    let mut outstanding: Option<Instant> = None;
    let mut last_dump = None;
    let begin = Instant::now();
    let mut tick = 0_u64;
    while begin.elapsed() < Duration::from_secs(args.secs) {
        let due = begin + period * tick as u32;
        tick += 1;
        if let Some(_since) = outstanding {
            missed += 1;
            run += 1;
            longest_run = longest_run.max(run);
        } else {
            run = 0;
            link.send(&Frame::get(Register::Dump))?;
            sent += 1;
            outstanding = Some(Instant::now());
        }
        while Instant::now() < due + period {
            for (seen, frame) in link.pump()? {
                if frame.register() == Some(Register::Dump) {
                    if let Some(at) = outstanding.take() {
                        latencies.push(seen - at);
                    }
                    reply_times.push(seen);
                    last_dump = Some(frame);
                } else {
                    *pushed.entry(frame.id()).or_insert(0) += 1;
                }
            }
        }
    }
    let elapsed = begin.elapsed();

    println!(
        "\npoll: GET FF at {} Hz for {:.1} s",
        args.hz,
        elapsed.as_secs_f64()
    );
    println!(
        "  ticks {tick}, GET sent {sent}, replies {}, ticks skipped (previous reply outstanding) {missed}",
        latencies.len()
    );
    println!(
        "  longest run of consecutive missed ticks: {longest_run} (link lost at {MISSED_TICKS}: {})",
        if longest_run >= MISSED_TICKS {
            "WOULD TRIP"
        } else {
            "no"
        }
    );
    if !latencies.is_empty() {
        latencies.sort();
        println!(
            "  reply latency ms: min {:.1}  median {:.1}  p95 {:.1}  p99 {:.1}  max {:.1}",
            millis(latencies[0]),
            millis(percentile(&latencies, 50.0)),
            millis(percentile(&latencies, 95.0)),
            millis(percentile(&latencies, 99.0)),
            millis(*latencies.last().unwrap()),
        );
    }
    let worst_gap = reply_times.windows(2).map(|w| w[1] - w[0]).max();
    if let Some(gap) = worst_gap {
        println!(
            "  worst gap between replies: {:.1} ms (a tick is {:.0} ms)",
            millis(gap),
            millis(period)
        );
    }
    print_rates("pushed frames while polling", &pushed, elapsed);
    let stats = link.reader.stats();
    println!("reader: {stats:?}");

    if let Some(last) = last_dump
        && let Ok(Reply::Dump(end)) = Reply::from_frame(&last)
        && let Ok(Reply::Dump(start)) = Reply::from_frame(&first)
    {
        println!(
            "unit setpoints unchanged by the probe: {}",
            start.set_voltage == end.set_voltage
                && start.set_current == end.set_current
                && start.output_on == end.output_on
        );
    }
    Ok(())
}
