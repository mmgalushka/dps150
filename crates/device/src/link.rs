//! The link state machine (APP.md §5, SAFETY.md §3). Pure: no port, thread or
//! clock. The caller passes the time with every call and moves bytes, as with
//! `SimDevice` (APP-11).
//!
//! In this stage the only frames it writes are the connect frame and GETs.

use std::time::Duration;

use protocol::{Dir, Dump, Frame, Reader, Register, Reply, Stats, Write};

use crate::constants::{
    DEFAULT_REFRESH_HZ, HANDSHAKE_RESEND, HANDSHAKE_TIMEOUT, MISSED_TICKS, RECONNECT_INTERVAL,
    SET_GAP,
};

/// Where the link is (APP.md §5). `Resolving` happens before the core exists.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Waiting for the transport to open the port.
    Opening,
    /// Port open; connect frame and first reads in progress.
    Handshake,
    /// First full dump received; polling.
    Online,
    /// Port closed; waiting to retry.
    Lost,
    /// Start-up failed. Terminal: the program exits with status 1 (APP-01).
    Failed,
}

/// Why the link went down or never came up.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cause {
    /// The port could not be opened.
    Open(OpenError),
    /// No `FF` reply within `HANDSHAKE_TIMEOUT`.
    NoAnswer,
    /// A read or write failed.
    Io(String),
    /// `MISSED_TICKS` poll ticks in a row without a reply.
    MissedPolls,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenError {
    pub reason: String,
    pub permission_denied: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// The first full dump of a session arrived. `restored` is false at start.
    Online { restored: bool },
    /// The link went down after having been online.
    Lost(Cause),
    /// Start-up failed (APP-01).
    Failed(Cause),
    /// The dump read after a SET: `written` is what was sent, `device` the
    /// same register as the device holds it (PRT-03).
    ReadBack { written: Write, device: Write },
    /// A write the core refused: a defect in the layer above (APP-12).
    WriteRefused { write: Write, why: Refusal },
}

/// Why `Link::write` said no.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Link down or no full dump yet (LNK-03, SAF-12).
    NotOnline,
    /// Outside the device range (APP-12).
    OutOfRange,
    /// An ON while an OFF is queued or not yet read back: the core never
    /// puts an ON behind an OFF the user asked for (SAF-01).
    OffPending,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LinkConfig {
    /// Poll rate: 10, 5 or 2 Hz (PRT-06).
    pub refresh_hz: u32,
    /// Retry every `RECONNECT_INTERVAL` when on; once per request when off.
    pub reconnect: bool,
}

impl Default for LinkConfig {
    fn default() -> Self {
        Self {
            refresh_hz: DEFAULT_REFRESH_HZ,
            reconnect: true,
        }
    }
}

/// What the app shows: the last full dump with pushed values merged in.
#[derive(Clone, Debug, PartialEq)]
pub struct Snapshot {
    pub dump: Dump,
    pub model: Option<String>,
    pub hardware: Option<String>,
    pub firmware: Option<String>,
}

/// What the link discarded or counted, for tests (APP-03).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LinkStats {
    pub reader: Stats,
    /// Frames that framed correctly but did not decode.
    pub decode_errors: u64,
    /// Poll ticks that found the previous reply still outstanding.
    pub missed_ticks: u64,
    /// Poll ticks that were quiet because of `SET_GAP`.
    pub quiet_ticks: u64,
    /// SETs put on the wire.
    pub sets_sent: u64,
    /// Writes refused by the second range check; a non-zero count is a defect
    /// (APP-12).
    pub refused_writes: u64,
}

pub struct Link {
    config: LinkConfig,
    state: State,
    reader: Reader,
    /// Counters of readers that were replaced on reconnect.
    carried: Stats,
    ever_online: bool,
    /// A `Lost` event is out and no `Online` has followed: failed retries
    /// are not new losses.
    loss_announced: bool,

    // Handshake.
    opened_at: Duration,
    connect_sent: bool,
    gets_sent: bool,
    resent: bool,

    // Polling.
    quiet_until: Duration,
    next_tick: Duration,
    outstanding: bool,
    missed: u32,

    retry_at: Option<Duration>,
    /// Writes waiting for their turn, one per register (the latest wins).
    pending: Vec<Write>,
    /// Written, waiting for the next poll to read them back.
    unchecked: Vec<Write>,
    /// Riding on the poll in flight.
    readback: Vec<Write>,
    dump: Option<Dump>,
    model: Option<String>,
    hardware: Option<String>,
    firmware: Option<String>,
    version: u64,
    events: Vec<Event>,
    stats: LinkStats,
}

impl Link {
    pub fn new(config: LinkConfig) -> Self {
        Self {
            config,
            state: State::Opening,
            reader: Reader::new(Dir::DeviceToHost),
            carried: Stats::default(),
            ever_online: false,
            loss_announced: false,
            opened_at: Duration::ZERO,
            connect_sent: false,
            gets_sent: false,
            resent: false,
            quiet_until: Duration::ZERO,
            next_tick: Duration::ZERO,
            outstanding: false,
            missed: 0,
            retry_at: None,
            pending: Vec::new(),
            unchecked: Vec::new(),
            readback: Vec::new(),
            dump: None,
            model: None,
            hardware: None,
            firmware: None,
            version: 0,
            events: Vec::new(),
            stats: LinkStats::default(),
        }
    }

    pub fn state(&self) -> State {
        self.state
    }

    pub fn config(&self) -> LinkConfig {
        self.config
    }

    pub fn set_refresh_hz(&mut self, hz: u32) {
        self.config.refresh_hz = hz.max(1);
    }

    pub fn set_reconnect(&mut self, on: bool) {
        self.config.reconnect = on;
    }

    /// Writes may only start once the first full dump has arrived (SAF-12).
    pub fn writes_allowed(&self) -> bool {
        self.state == State::Online
    }

    /// The last known device state; `None` before the first dump. While the
    /// link is not `Online` the numbers are stale and must not be shown live
    /// (LNK-02).
    pub fn snapshot(&self) -> Option<Snapshot> {
        self.dump.clone().map(|dump| Snapshot {
            dump,
            model: self.model.clone(),
            hardware: self.hardware.clone(),
            firmware: self.firmware.clone(),
        })
    }

    /// Increases whenever the snapshot changes.
    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn stats(&self) -> LinkStats {
        let now = self.reader.stats();
        LinkStats {
            reader: Stats {
                bad_checksum: self.carried.bad_checksum + now.bad_checksum,
                bad_length: self.carried.bad_length + now.bad_length,
                skipped_bytes: self.carried.skipped_bytes + now.skipped_bytes,
                stale_frames: self.carried.stale_frames + now.stale_frames,
            },
            ..self.stats
        }
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    // ---- writes -------------------------------------------------------------

    /// Queues a write. Only an online link with a full dump accepts one, and
    /// only a value inside the device range (APP-12). A second write to the
    /// same register replaces the first before it is sent; at most one SET
    /// goes out per `SET_GAP`, always the oldest register first.
    pub fn write(&mut self, write: Write) -> Result<(), Refusal> {
        let refuse = |this: &mut Self, why| {
            this.stats.refused_writes += 1;
            this.events.push(Event::WriteRefused { write, why });
            Err(why)
        };
        let Some(dump) = self.dump.as_ref().filter(|_| self.state == State::Online) else {
            return refuse(self, Refusal::NotOnline);
        };
        let limit = match write {
            Write::SetVoltage(_) | Write::PresetVoltage(..) => dump.ovp_range.get(),
            Write::SetCurrent(_) | Write::PresetCurrent(..) => dump.max_current.get(),
            Write::Ovp(_) => dump.ovp_range.get(),
            Write::Ocp(_) => dump.ocp_range.get(),
            Write::Opp(_) => dump.opp_range.get(),
            Write::Otp(_) => dump.otp_range.get(),
            Write::Lvp(_) => dump.lvp_range.get(),
            Write::Output(_) => u32::MAX,
        };
        if write.thousandths() > limit {
            return refuse(self, Refusal::OutOfRange);
        }
        match write {
            // An OFF goes first, past any queued setpoint, and is never
            // merged away or dropped (APP-09). A queued ON is void.
            Write::Output(false) => {
                self.pending
                    .retain(|queued| queued.register() != write.register());
                self.pending.insert(0, write);
            }
            Write::Output(true) => {
                let off_waiting = [&self.pending, &self.unchecked, &self.readback]
                    .into_iter()
                    .any(|list| list.contains(&Write::Output(false)));
                if off_waiting {
                    return refuse(self, Refusal::OffPending);
                }
                self.pending
                    .retain(|queued| queued.register() != write.register());
                // Behind every setpoint already queued, so the ON happens at
                // the values the user saw.
                self.pending.push(write);
            }
            _ => {
                self.pending
                    .retain(|queued| queued.register() != write.register());
                self.pending.push(write);
            }
        }
        Ok(())
    }

    /// Writes queued and not yet on the wire.
    pub fn pending_writes(&self) -> &[Write] {
        &self.pending
    }

    // ---- transport reports ------------------------------------------------

    /// Should the transport try to open the port now?
    pub fn wants_open(&self, now: Duration) -> bool {
        match self.state {
            State::Opening => true,
            State::Lost => self.retry_at.is_some_and(|at| now >= at),
            _ => false,
        }
    }

    /// With Reconnect off, the user asked for one attempt (`r`, LNK-06).
    pub fn request_reconnect(&mut self, now: Duration) {
        if self.state == State::Lost {
            self.retry_at = Some(now);
        }
    }

    /// The port is open: the handshake starts.
    pub fn port_opened(&mut self, now: Duration) {
        if !matches!(self.state, State::Opening | State::Lost) {
            return;
        }
        self.state = State::Handshake;
        self.reset_reader();
        self.opened_at = now;
        self.connect_sent = false;
        self.gets_sent = false;
        self.resent = false;
        self.outstanding = false;
        self.missed = 0;
        self.retry_at = None;
        self.dump = None;
    }

    pub fn open_failed(&mut self, now: Duration, error: OpenError) {
        if matches!(self.state, State::Opening | State::Lost) {
            self.lose(now, Cause::Open(error));
        }
    }

    /// A read or write on the port failed: the link is lost at once.
    pub fn io_error(&mut self, now: Duration, reason: impl Into<String>) {
        if matches!(self.state, State::Handshake | State::Online) {
            self.lose(now, Cause::Io(reason.into()));
        }
    }

    // ---- bytes and time ---------------------------------------------------

    /// Takes bytes read from the port.
    pub fn feed(&mut self, now: Duration, bytes: &[u8]) {
        if !matches!(self.state, State::Handshake | State::Online) {
            return;
        }
        for frame in self.reader.feed(now, bytes) {
            match Reply::from_frame(&frame) {
                Ok(reply) => self.apply(now, reply),
                Err(protocol::DecodeError::UnknownRegister(_)) => {}
                Err(_) => self.stats.decode_errors += 1,
            }
        }
    }

    /// Moves time forward and returns the bytes to write now.
    pub fn poll(&mut self, now: Duration) -> Vec<u8> {
        // Lets the reader drop a stale partial frame (APP-04).
        self.reader.feed(now, &[]);
        match self.state {
            State::Handshake => self.poll_handshake(now),
            State::Online => self.poll_online(now),
            _ => Vec::new(),
        }
    }

    fn poll_handshake(&mut self, now: Duration) -> Vec<u8> {
        if now.saturating_sub(self.opened_at) >= HANDSHAKE_TIMEOUT {
            self.lose(now, Cause::NoAnswer);
            return Vec::new();
        }
        if !self.resent && now.saturating_sub(self.opened_at) >= HANDSHAKE_RESEND {
            self.resent = true;
            self.connect_sent = false;
            self.gets_sent = false;
        }
        let mut out = Vec::new();
        if !self.connect_sent {
            out.extend(Frame::connect().encode());
            self.connect_sent = true;
            self.quiet_until = now + SET_GAP;
        }
        if !self.gets_sent && now >= self.quiet_until {
            for register in [
                Register::Model,
                Register::HardwareVersion,
                Register::FirmwareVersion,
                Register::Dump,
            ] {
                out.extend(Frame::get(register).encode());
            }
            self.gets_sent = true;
        }
        out
    }

    fn poll_online(&mut self, now: Duration) -> Vec<u8> {
        let mut out = Vec::new();
        // One SET per gap: nothing may follow it for `SET_GAP`, and the ticks
        // below are quiet until then (SAF-15).
        if now >= self.quiet_until && !self.pending.is_empty() {
            let write = self.pending.remove(0);
            out.extend(write.to_frame().encode());
            self.stats.sets_sent += 1;
            self.quiet_until = now + SET_GAP;
            self.unchecked.push(write);
        }
        let period = self.period();
        while now >= self.next_tick {
            let tick = self.next_tick;
            self.next_tick += period;
            if tick < self.quiet_until {
                // Quiet after a write: not a missed poll (SAF-15).
                self.stats.quiet_ticks += 1;
            } else if self.outstanding {
                self.stats.missed_ticks += 1;
                self.missed += 1;
                if self.missed >= MISSED_TICKS {
                    self.lose(now, Cause::MissedPolls);
                    return Vec::new();
                }
            } else {
                out.extend(Frame::get(Register::Dump).encode());
                self.outstanding = true;
                // This poll goes out after the gap, so its dump shows what
                // the device made of the writes (PRT-03).
                self.readback.append(&mut self.unchecked);
            }
        }
        out
    }

    // ---- model ------------------------------------------------------------

    fn apply(&mut self, now: Duration, reply: Reply) {
        match reply {
            Reply::Dump(dump) => {
                self.dump = Some(*dump);
                if let Some(dump) = self.dump.as_ref() {
                    for written in std::mem::take(&mut self.readback) {
                        let device = device_value(dump, written);
                        self.events.push(Event::ReadBack { written, device });
                    }
                }
                self.version += 1;
                self.outstanding = false;
                self.missed = 0;
                if self.state == State::Handshake {
                    self.go_online(now);
                }
            }
            Reply::Model(text) => self.model = Some(text),
            Reply::HardwareVersion(text) => self.hardware = Some(text),
            Reply::FirmwareVersion(text) => self.firmware = Some(text),
            // Pushed frames update the model whenever they arrive; they do not
            // answer a poll (PRT-06).
            Reply::InputVoltage(value) => self.patch(|dump| dump.input_voltage = value),
            Reply::Output(value) => self.patch(|dump| dump.output = value),
            Reply::Temperature(value) => self.patch(|dump| dump.temperature = value),
            Reply::MaxVoltage(value) => self.patch(|dump| dump.max_voltage = value),
            Reply::MaxCurrent(value) => self.patch(|dump| dump.max_current = value),
            _ => {}
        }
    }

    fn patch(&mut self, change: impl FnOnce(&mut Dump)) {
        if let Some(dump) = self.dump.as_mut() {
            let before = dump.clone();
            change(dump);
            if *dump != before {
                self.version += 1;
            }
        }
    }

    fn reset_reader(&mut self) {
        let old = self.reader.stats();
        self.carried = Stats {
            bad_checksum: self.carried.bad_checksum + old.bad_checksum,
            bad_length: self.carried.bad_length + old.bad_length,
            skipped_bytes: self.carried.skipped_bytes + old.skipped_bytes,
            stale_frames: self.carried.stale_frames + old.stale_frames,
        };
        self.reader = Reader::new(Dir::DeviceToHost);
    }

    fn go_online(&mut self, now: Duration) {
        let restored = self.ever_online;
        self.ever_online = true;
        self.loss_announced = false;
        self.state = State::Online;
        // The first poll is one period after the handshake reply.
        self.next_tick = now + self.period();
        self.events.push(Event::Online { restored });
    }

    fn period(&self) -> Duration {
        Duration::from_secs_f64(1.0 / f64::from(self.config.refresh_hz.max(1)))
    }

    fn lose(&mut self, now: Duration, cause: Cause) {
        // Nothing queued survives the loss (LNK-03).
        self.pending.clear();
        self.unchecked.clear();
        self.readback.clear();
        self.outstanding = false;
        self.missed = 0;
        self.reset_reader();
        if self.ever_online {
            // A failed retry from `Lost` is not a new loss.
            if !self.loss_announced {
                self.loss_announced = true;
                self.events.push(Event::Lost(cause));
            }
            self.state = State::Lost;
            self.retry_at = self.config.reconnect.then_some(now + RECONNECT_INTERVAL);
        } else {
            self.state = State::Failed;
            self.events.push(Event::Failed(cause));
        }
    }
}

/// The value the device holds for the register `written` went to.
fn device_value(dump: &Dump, written: Write) -> Write {
    let slot_index = |slot: protocol::Slot| usize::from(slot.number() - 1);
    match written {
        Write::SetVoltage(_) => Write::SetVoltage(dump.set_voltage),
        Write::SetCurrent(_) => Write::SetCurrent(dump.set_current),
        Write::PresetVoltage(slot, _) => {
            Write::PresetVoltage(slot, dump.presets[slot_index(slot)].voltage)
        }
        Write::PresetCurrent(slot, _) => {
            Write::PresetCurrent(slot, dump.presets[slot_index(slot)].current)
        }
        Write::Ovp(_) => Write::Ovp(dump.ovp),
        Write::Ocp(_) => Write::Ocp(dump.ocp),
        Write::Opp(_) => Write::Opp(dump.opp),
        Write::Otp(_) => Write::Otp(dump.otp),
        Write::Lvp(_) => Write::Lvp(dump.lvp),
        Write::Output(_) => Write::Output(dump.output_on),
    }
}
