//! The device thread (APP-08): it owns the serial port, reads and writes
//! bytes, and calls the pure [`Link`]. All link logic stays in the core.

use std::fmt;
use std::io::{self, Read, Write};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use serialport::{DataBits, FlowControl, Parity, SerialPort, StopBits};

use crate::constants::BAUD;
use protocol::Write as Setting;

use crate::link::{Cause, Event, Link, LinkConfig, OpenError, Snapshot, State};

/// How long one read waits; also the pace of the thread.
const READ_TIMEOUT: Duration = Duration::from_millis(5);
/// Pace while the port is closed and a retry is pending.
const IDLE: Duration = Duration::from_millis(10);

/// UI to device thread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    /// One reconnect attempt (`r`, used when Reconnect is off, LNK-06).
    Reconnect,
    /// Poll rate in hertz.
    Refresh(u32),
    /// Retry automatically while the link is down.
    AutoReconnect(bool),
    /// A setpoint, preset or limit write. The core checks it again (APP-12).
    Write(Setting),
    /// Quit: send what is queued, then the close frame (SAF-14). The thread
    /// answers with `Event::Closed`.
    Close,
}

/// Device thread to UI.
#[derive(Clone, Debug, PartialEq)]
pub enum Message {
    Event(Event),
    /// The model changed; this is the new snapshot.
    Model(Box<Snapshot>),
}

/// Why the program cannot start (APP-01). `Display` is the stderr message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupError {
    pub port: String,
    pub cause: Cause,
}

impl fmt::Display for StartupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let port = &self.port;
        match &self.cause {
            Cause::Open(error) => {
                write!(f, "dps150: cannot open {port}: {}", error.reason)?;
                if error.permission_denied {
                    write!(f, "\nLinux: add your user to the dialout group")?;
                }
                Ok(())
            }
            Cause::NoAnswer => write!(f, "dps150: device did not answer on {port}"),
            Cause::Io(reason) => write!(f, "dps150: cannot talk to {port}: {reason}"),
            Cause::MissedPolls => write!(f, "dps150: lost the device on {port}"),
        }
    }
}

impl std::error::Error for StartupError {}

/// Handle to the device thread. Dropping it stops the thread and closes the
/// port without sending anything.
pub struct Device {
    commands: Sender<Command>,
    messages: Receiver<Message>,
    stop: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl Device {
    /// Opens `port`, runs the handshake and waits for the first full dump.
    /// Fails with the message APP-01 asks for; there is no start screen.
    pub fn start(port: &str, config: LinkConfig) -> Result<Self, StartupError> {
        let (commands_tx, commands_rx) = mpsc::channel();
        let (messages_tx, messages_rx) = mpsc::channel();
        let (stop_tx, stop_rx) = mpsc::channel();
        let (started_tx, started_rx) = mpsc::channel();
        let path = port.to_string();
        let thread = thread::Builder::new()
            .name("device".into())
            .spawn(move || {
                run(
                    &path,
                    config,
                    &commands_rx,
                    &stop_rx,
                    &messages_tx,
                    &started_tx,
                )
            })
            .map_err(|error| StartupError {
                port: port.to_string(),
                cause: Cause::Io(error.to_string()),
            })?;
        let device = Self {
            commands: commands_tx,
            messages: messages_rx,
            stop: stop_tx,
            thread: Some(thread),
        };
        match started_rx.recv() {
            Ok(Ok(())) => Ok(device),
            Ok(Err(cause)) => Err(StartupError {
                port: port.to_string(),
                cause,
            }),
            Err(_) => Err(StartupError {
                port: port.to_string(),
                cause: Cause::Io("the device thread ended".into()),
            }),
        }
    }

    pub fn messages(&self) -> &Receiver<Message> {
        &self.messages
    }

    pub fn send(&self, command: Command) {
        let _ = self.commands.send(command);
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

fn open(path: &str) -> Result<Box<dyn SerialPort>, OpenError> {
    serialport::new(path, BAUD)
        .data_bits(DataBits::Eight)
        .parity(Parity::None)
        .stop_bits(StopBits::One)
        .flow_control(FlowControl::None)
        .timeout(READ_TIMEOUT)
        .open()
        .map_err(|error| OpenError {
            permission_denied: matches!(
                error.kind(),
                serialport::ErrorKind::Io(io::ErrorKind::PermissionDenied)
            ),
            reason: error.description,
        })
}

fn run(
    path: &str,
    config: LinkConfig,
    commands: &Receiver<Command>,
    stop: &Receiver<()>,
    messages: &Sender<Message>,
    started: &Sender<Result<(), Cause>>,
) {
    let begin = Instant::now();
    let mut link = Link::new(config);
    let mut port: Option<Box<dyn SerialPort>> = None;
    let mut started = Some(started);
    let mut sent_version = 0;
    let mut buffer = [0u8; 512];

    loop {
        if !matches!(stop.try_recv(), Err(TryRecvError::Empty)) {
            return;
        }
        while let Ok(command) = commands.try_recv() {
            match command {
                Command::Reconnect => link.request_reconnect(begin.elapsed()),
                Command::Refresh(hz) => link.set_refresh_hz(hz),
                Command::AutoReconnect(on) => link.set_reconnect(on),
                // A refusal comes back as an event.
                Command::Write(write) => drop(link.write(write)),
                Command::Close => link.close(),
            }
        }

        let now = begin.elapsed();
        if link.wants_open(now) {
            match open(path) {
                Ok(opened) => {
                    port = Some(opened);
                    link.port_opened(now);
                }
                Err(error) => link.open_failed(now, error),
            }
        }

        if let Some(open_port) = port.as_mut() {
            match open_port.read(&mut buffer) {
                Ok(0) => link.io_error(begin.elapsed(), "port closed"),
                Ok(count) => link.feed(begin.elapsed(), &buffer[..count]),
                Err(error)
                    if matches!(
                        error.kind(),
                        io::ErrorKind::TimedOut
                            | io::ErrorKind::WouldBlock
                            | io::ErrorKind::Interrupted
                    ) => {}
                Err(error) => link.io_error(begin.elapsed(), error.to_string()),
            }
            let out = link.poll(begin.elapsed());
            if !out.is_empty()
                && let Err(error) = open_port.write_all(&out).and_then(|()| open_port.flush())
            {
                link.io_error(begin.elapsed(), error.to_string());
            }
        } else {
            thread::sleep(IDLE);
        }

        if !matches!(link.state(), State::Handshake | State::Online) {
            port = None;
        }

        if link.version() != sent_version {
            sent_version = link.version();
            if let Some(snapshot) = link.snapshot() {
                let _ = messages.send(Message::Model(Box::new(snapshot)));
            }
        }
        for event in link.take_events() {
            // The first outcome answers `Device::start`; it is not repeated
            // on the message channel.
            if let Some(signal) = started.take() {
                let _ = match event {
                    Event::Online { .. } => signal.send(Ok(())),
                    Event::Failed(cause) | Event::Lost(cause) => signal.send(Err(cause)),
                    _ => Ok(()),
                };
                continue;
            }
            let _ = messages.send(Message::Event(event));
        }
        if link.state() == State::Failed {
            return;
        }
    }
}
