//! `dps150-sim`: the simulator on a pseudo-terminal (SIM-03).
//!
//! Prints `dps150-sim: listening on <path>` first, then serves the protocol
//! on that pty in real time and reads control commands from stdin (SIM-11).
//! Run the app against it with `dps150 --port <path>`.
#![cfg(unix)]

use std::io::{self, BufRead, Read, Write};
use std::process::ExitCode;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use serialport::{SerialPort, TTYPort};
use simulator::{Command, Outcome, SimDevice};

/// How long one read of the pty waits; also the loop's pace.
const TICK: Duration = Duration::from_millis(2);

/// Both ends are kept so the pty survives the app closing its side: with no
/// slave open, the master would read errors until the next open.
struct Pty {
    master: TTYPort,
    _slave: TTYPort,
}

fn open_pty() -> Result<(Pty, String), serialport::Error> {
    let (mut master, slave) = TTYPort::pair()?;
    master.set_timeout(TICK)?;
    let path = slave.name().ok_or_else(|| {
        serialport::Error::new(serialport::ErrorKind::Unknown, "the pty has no path")
    })?;
    Ok((
        Pty {
            master,
            _slave: slave,
        },
        path,
    ))
}

fn announce(path: &str) {
    println!("dps150-sim: listening on {path}");
    let _ = io::stdout().flush();
}

fn reply(text: &str) {
    println!("{text}");
    let _ = io::stdout().flush();
}

fn main() -> ExitCode {
    let (opened, path) = match open_pty() {
        Ok(opened) => opened,
        Err(error) => {
            eprintln!("dps150-sim: cannot open a pty: {error}");
            return ExitCode::FAILURE;
        }
    };
    let mut pty = Some(opened);
    announce(&path);

    let (lines_tx, lines_rx) = mpsc::channel::<String>();
    thread::spawn(move || {
        for line in io::stdin().lock().lines().map_while(Result::ok) {
            if lines_tx.send(line).is_err() {
                break;
            }
        }
    });

    let mut sim = SimDevice::new();
    let start = Instant::now();
    let mut buffer = [0u8; 256];
    loop {
        while let Ok(line) = lines_rx.try_recv() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            match line.parse::<Command>() {
                Err(error) => reply(&format!("error: {error}")),
                Ok(command) => match sim.apply(command) {
                    Outcome::Done => reply("ok"),
                    Outcome::Text(text) => {
                        print!("{text}");
                        reply("ok");
                    }
                    Outcome::Close => {
                        pty = None;
                        reply("ok");
                    }
                    Outcome::Reopen => match open_pty() {
                        Ok((opened, path)) => {
                            pty = Some(opened);
                            announce(&path);
                            reply("ok");
                        }
                        Err(error) => reply(&format!("error: {error}")),
                    },
                    Outcome::Quit => {
                        reply("ok");
                        return ExitCode::SUCCESS;
                    }
                },
            }
        }

        sim.advance(start.elapsed().saturating_sub(sim.now()));
        let Some(open) = pty.as_mut() else {
            thread::sleep(TICK);
            continue;
        };
        // An error is a timeout, or the app has the port closed: nothing to read.
        if let Ok(count) = open.master.read(&mut buffer) {
            sim.feed(&buffer[..count]);
        }
        let output = sim.take_output();
        if !output.is_empty() {
            let _ = open.master.write_all(&output);
        }
    }
}
