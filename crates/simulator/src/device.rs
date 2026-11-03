//! The device model (SIMULATOR.md §3 and §4).

use std::collections::HashMap;
use std::fmt::Write as _;
use std::time::Duration;

use protocol::{Dir, Frame, Reader, Register, Slot, command};

/// A SET, connect or close frame is lost when another frame follows within
/// this long (SIM-14). The real window is 0 to 5 ms; the length is assumed.
pub const SET_DROP_WINDOW: Duration = Duration::from_millis(2);

/// One push frame every 125 ms (SIMULATOR.md §3).
const PUSH_PERIOD: Duration = Duration::from_millis(125);
/// The push order seen in the capture.
const PUSH_CYCLE: [u8; 5] = [0xC3, 0xC0, 0xE2, 0xE3, 0xC4];
/// Gap between bytes of a `split` frame.
const SPLIT_GAP: Duration = Duration::from_millis(20);
/// Longest step of the output model.
const MAX_STEP: Duration = Duration::from_millis(5);
/// The output voltage reaches its setpoint in about this long [V].
const RAMP_UP: f64 = 1.0;
/// The output voltage decays in about this long [V].
const RAMP_DOWN: f64 = 0.5;
/// The delivered voltage stays this far below the live maximum `E2` (19.98 V
/// reported for 20.05 V) [V, observed once].
const CLAMP_OFFSET: f64 = 0.07;
/// `E2 = Vin - LIVE_MAX_OFFSET` [V, near].
const LIVE_MAX_OFFSET: f64 = 0.2;
const MODEL: &[u8] = b"DPS-150";
const HARDWARE: &[u8] = b"V1.0";
const FIRMWARE: &[u8] = b"V1.2";

/// What the protection register `DC` reports (SIM-10).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protection {
    None,
    Ovp,
    Ocp,
    Opp,
    Otp,
    Lvp,
}

impl Protection {
    pub fn code(self) -> u8 {
        match self {
            Self::None => 0,
            Self::Ovp => 1,
            Self::Ocp => 2,
            Self::Opp => 3,
            Self::Otp => 4,
            Self::Lvp => 5,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Ovp => "ovp",
            Self::Ocp => "ocp",
            Self::Opp => "opp",
            Self::Otp => "otp",
            Self::Lvp => "lvp",
        }
    }
}

/// What is connected to the output.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Load {
    Open,
    Ohms(f64),
}

#[derive(Debug)]
enum Action {
    Connect,
    Close,
    Set(Frame),
}

#[derive(Debug)]
struct Pending {
    action: Action,
    deadline: Duration,
}

/// The simulated supply.
pub struct SimDevice {
    now: Duration,
    reader: Reader,
    pending: Option<Pending>,

    input_voltage: f32,
    set_voltage: f32,
    set_current: f32,
    presets: [(f32, f32); Slot::COUNT],
    ovp: f32,
    ocp: f32,
    opp: f32,
    otp: f32,
    lvp: f32,
    brightness: u8,
    volume: u8,
    temperature: f32,
    live_max: Option<f32>,
    max_current: f32,
    ranges: [f32; 5],

    load: Load,
    output_on: bool,
    /// Fraction of the target voltage that is delivered, 0 to 1.
    level: f64,
    protection: Protection,
    capacity: f64,
    energy: f64,

    connected: bool,
    locked: bool,
    unplugged: bool,
    next_push: Duration,
    push_index: usize,

    outbox: Vec<(Duration, Vec<u8>)>,
    keep: HashMap<u8, f64>,
    corrupt: u32,
    split: u32,
    /// When the last byte of the last split frame goes out; later frames
    /// wait for it, as bytes on a serial line do not interleave.
    split_end: Duration,
    delay: Duration,
    dropped: u64,
}

impl Default for SimDevice {
    fn default() -> Self {
        Self::new()
    }
}

impl SimDevice {
    /// The state observed on the test unit (SIM-04).
    pub fn new() -> Self {
        Self {
            now: Duration::ZERO,
            reader: Reader::new(Dir::HostToDevice),
            pending: None,
            input_voltage: 20.234,
            set_voltage: 3.3,
            set_current: 2.0,
            presets: [(5.0, 1.0); Slot::COUNT],
            ovp: 30.0,
            ocp: 5.1,
            opp: 150.0,
            otp: 80.0,
            lvp: 5.0,
            brightness: 3,
            volume: 1,
            temperature: 23.608,
            live_max: None,
            max_current: 5.1,
            ranges: [30.0, 5.1, 150.0, 99.0, 30.0],
            load: Load::Ohms(10.0),
            output_on: false,
            level: 0.0,
            protection: Protection::None,
            capacity: 0.0,
            energy: 0.0,
            connected: false,
            locked: false,
            unplugged: false,
            next_push: Duration::ZERO,
            push_index: 0,
            outbox: Vec::new(),
            keep: HashMap::new(),
            corrupt: 0,
            split: 0,
            split_end: Duration::ZERO,
            delay: Duration::ZERO,
            dropped: 0,
        }
    }

    // ---- bytes and time -------------------------------------------------

    /// Takes what the host wrote. Frames may be split anywhere or joined.
    pub fn feed(&mut self, bytes: &[u8]) {
        if self.unplugged || bytes.is_empty() {
            return;
        }
        self.settle();
        let had_pending = self.pending.take();
        if had_pending.is_some() {
            // Another frame arrived inside the window: the SET is lost.
            self.dropped += 1;
        }
        let frames = self.reader.feed(self.now, bytes);
        let count = frames.len();
        for (index, frame) in frames.into_iter().enumerate() {
            let last = index + 1 == count && self.reader.pending() == 0;
            self.handle(frame, last);
        }
        // A frame still half received is also "another frame" for a SET that
        // came just before it in the same write.
        if self.reader.pending() > 0 && self.pending.take().is_some() {
            self.dropped += 1;
        }
    }

    /// Moves simulated time forward.
    pub fn advance(&mut self, dt: Duration) {
        let end = self.now + dt;
        while self.now < end {
            let mut step = (end - self.now).min(MAX_STEP);
            if let Some(pending) = &self.pending {
                step = step.min(
                    pending
                        .deadline
                        .saturating_sub(self.now)
                        .max(Duration::from_nanos(1)),
                );
            }
            if self.session_open() && self.next_push > self.now {
                step = step.min(self.next_push - self.now);
            }
            self.now += step;
            self.settle();
            self.step_output(step.as_secs_f64());
            self.push_due();
        }
    }

    /// The bytes the device has sent by now.
    pub fn take_output(&mut self) -> Vec<u8> {
        let now = self.now;
        let (mut ready, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.outbox)
            .into_iter()
            .partition(|(due, _)| *due <= now);
        self.outbox = rest;
        ready.sort_by_key(|(due, _)| *due);
        ready.into_iter().flat_map(|(_, bytes)| bytes).collect()
    }

    pub fn now(&self) -> Duration {
        self.now
    }

    // ---- fault injection (SIMULATOR.md §5) ------------------------------

    pub fn set_load(&mut self, load: Load) {
        self.load = load;
    }

    pub fn set_input_voltage(&mut self, volts: f32) {
        self.input_voltage = volts;
    }

    pub fn set_temperature(&mut self, celsius: f32) {
        self.temperature = celsius;
    }

    /// Fixes `E2` instead of deriving it from the input voltage.
    pub fn set_live_max(&mut self, volts: Option<f32>) {
        self.live_max = volts;
    }

    pub fn set_max_current(&mut self, amps: f32) {
        self.max_current = amps;
    }

    /// Forces a protection to trip now.
    pub fn trip(&mut self, protection: Protection) {
        if protection != Protection::None {
            self.protection = protection;
            self.output_on = false;
        }
    }

    /// The device keeps `value` for `id` instead of what a SET carries.
    pub fn keep(&mut self, id: u8, value: f64) {
        self.keep.insert(id, value);
    }

    /// The device goes silent and ignores input; the output state is kept.
    pub fn unplug(&mut self) {
        self.unplugged = true;
        self.connected = false;
        self.locked = false;
        self.pending = None;
        self.outbox.clear();
        self.split_end = Duration::ZERO;
    }

    /// The device answers again, but only after a new connect frame.
    pub fn plug(&mut self) {
        self.unplugged = false;
    }

    pub fn corrupt(&mut self, frames: u32) {
        self.corrupt = frames;
    }

    pub fn split(&mut self, frames: u32) {
        self.split = frames;
    }

    pub fn delay(&mut self, delay: Duration) {
        self.delay = delay;
    }

    /// Resets counters and the session; settings stay (SIM-04, §4).
    pub fn powercycle(&mut self) {
        self.capacity = 0.0;
        self.energy = 0.0;
        self.output_on = false;
        self.level = 0.0;
        self.protection = Protection::None;
        self.connected = false;
        self.locked = false;
        self.pending = None;
        self.outbox.clear();
        self.split_end = Duration::ZERO;
        self.reader = Reader::new(Dir::HostToDevice);
    }

    // ---- inspection -----------------------------------------------------

    pub fn is_locked(&self) -> bool {
        self.locked
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn is_output_on(&self) -> bool {
        self.output_on
    }

    pub fn protection(&self) -> Protection {
        self.protection
    }

    pub fn set_voltage(&self) -> f32 {
        self.set_voltage
    }

    pub fn set_current(&self) -> f32 {
        self.set_current
    }

    /// SETs and connect/close frames lost to the write gap so far.
    pub fn dropped(&self) -> u64 {
        self.dropped
    }

    /// Delivered voltage, current and power right now.
    pub fn measurement(&self) -> (f32, f32, f32) {
        let (v, i, p) = self.measure();
        (v as f32, i as f32, p as f32)
    }

    /// One `name value` per line (SIM-11).
    pub fn state(&self) -> String {
        let (v, i, p) = self.measurement();
        let mut text = String::new();
        let mut line = |name: &str, value: String| {
            let _ = writeln!(text, "{name} {value}");
        };
        line("connected", self.connected.to_string());
        line("locked", self.locked.to_string());
        line("unplugged", self.unplugged.to_string());
        line("output", if self.output_on { "on" } else { "off" }.into());
        line("mode", if self.is_cc() { "cc" } else { "cv" }.into());
        line("protection", self.protection.name().into());
        line("vset", self.set_voltage.to_string());
        line("iset", self.set_current.to_string());
        line("v", v.to_string());
        line("i", i.to_string());
        line("p", p.to_string());
        line("vin", self.input_voltage.to_string());
        line("emax", self.live_max_voltage().to_string());
        line("temp", self.temperature.to_string());
        line(
            "load",
            match self.load {
                Load::Open => "open".into(),
                Load::Ohms(ohms) => ohms.to_string(),
            },
        );
        line("ah", (self.capacity as f32).to_string());
        line("wh", (self.energy as f32).to_string());
        line("ovp", self.ovp.to_string());
        line("ocp", self.ocp.to_string());
        line("opp", self.opp.to_string());
        line("otp", self.otp.to_string());
        line("lvp", self.lvp.to_string());
        line("brightness", self.brightness.to_string());
        line("volume", self.volume.to_string());
        line("dropped", self.dropped.to_string());
        text
    }

    // ---- frames in ------------------------------------------------------

    fn session_open(&self) -> bool {
        self.connected && !self.unplugged
    }

    /// Applies a held SET whose window has passed without another frame.
    fn settle(&mut self) {
        if self
            .pending
            .as_ref()
            .is_some_and(|pending| self.now >= pending.deadline)
        {
            let pending = self.pending.take().expect("checked above");
            match pending.action {
                Action::Connect => {
                    if !self.connected {
                        self.connected = true;
                        self.next_push = self.now + PUSH_PERIOD;
                        self.push_index = 0;
                    }
                    self.locked = true;
                }
                Action::Close => {
                    self.connected = false;
                    self.locked = false;
                }
                Action::Set(frame) => self.apply_set(&frame),
            }
        }
    }

    fn handle(&mut self, frame: Frame, last: bool) {
        let action = match frame.command() {
            command::CONNECT if frame.id() == 0 => match frame.data() {
                [1] => Some(Action::Connect),
                [0] => Some(Action::Close),
                _ => None,
            },
            command::SET if self.connected => Some(Action::Set(frame.clone())),
            command::DATA if self.connected => {
                self.answer(&frame);
                None
            }
            _ => None,
        };
        let Some(action) = action else { return };
        if last {
            self.pending = Some(Pending {
                action,
                deadline: self.now + SET_DROP_WINDOW,
            });
        } else {
            // Another frame follows at once: this one is lost.
            self.dropped += 1;
        }
    }

    fn answer(&mut self, frame: &Frame) {
        let Some(register) = frame.register() else {
            return;
        };
        let Some(data) = self.read(register) else {
            return;
        };
        let reply = Frame::new(Dir::DeviceToHost, command::DATA, frame.id(), data)
            .expect("replies are short");
        self.send(&reply, self.now + self.delay);
    }

    fn apply_set(&mut self, frame: &Frame) {
        let Some(register) = frame.register() else {
            return;
        };
        let id = frame.id();
        let float = |this: &Self| this.kept(id, float_of(frame.data()) as f64) as f32;
        let byte = |this: &Self| this.kept(id, f64::from(frame.data()[0])) as u8;
        match register {
            Register::SetVoltage => self.set_voltage = float(self),
            Register::SetCurrent => self.set_current = float(self),
            Register::PresetVoltage(slot) => {
                self.presets[usize::from(slot.number() - 1)].0 = float(self);
            }
            Register::PresetCurrent(slot) => {
                self.presets[usize::from(slot.number() - 1)].1 = float(self);
            }
            Register::Ovp => self.ovp = float(self),
            Register::Ocp => self.ocp = float(self),
            Register::Opp => self.opp = float(self),
            Register::Otp => self.otp = float(self),
            Register::Lvp => self.lvp = float(self),
            Register::Brightness => self.brightness = byte(self) & 0x0F,
            Register::Volume => self.volume = byte(self) & 0x0F,
            Register::OutputState => match byte(self) {
                1 => {
                    // Clears a latched trip and turns the output on (SIM-09).
                    self.protection = Protection::None;
                    self.output_on = true;
                }
                0 => self.output_on = false,
                _ => {}
            },
            // Read-only and unknown types are ignored (SIM-13).
            _ => {}
        }
    }

    fn kept(&self, id: u8, written: f64) -> f64 {
        self.keep.get(&id).copied().unwrap_or(written)
    }

    // ---- frames out -----------------------------------------------------

    fn send(&mut self, frame: &Frame, due: Duration) {
        let due = due.max(self.split_end);
        let mut bytes = frame.encode();
        if self.corrupt > 0 {
            self.corrupt -= 1;
            if let Some(checksum) = bytes.last_mut() {
                *checksum = checksum.wrapping_add(1);
            }
        }
        if self.split > 0 {
            self.split -= 1;
            self.split_end = due + SPLIT_GAP * (bytes.len() as u32 - 1);
            for (index, byte) in bytes.into_iter().enumerate() {
                self.outbox
                    .push((due + SPLIT_GAP * index as u32, vec![byte]));
            }
        } else {
            self.outbox.push((due, bytes));
        }
    }

    fn push_due(&mut self) {
        while self.session_open() && self.now >= self.next_push {
            let id = PUSH_CYCLE[self.push_index % PUSH_CYCLE.len()];
            self.push_index += 1;
            let due = self.next_push;
            self.next_push += PUSH_PERIOD;
            let register = Register::from_id(id).expect("cycle holds known types");
            let data = self.read(register).expect("cycle holds readable types");
            let frame =
                Frame::new(Dir::DeviceToHost, command::DATA, id, data).expect("short frame");
            self.send(&frame, due);
        }
    }

    /// The data bytes of a GET reply; `None` for a type that gets no reply.
    fn read(&self, register: Register) -> Option<Vec<u8>> {
        let float = |value: f32| value.to_le_bytes().to_vec();
        Some(match register {
            Register::InputVoltage => float(self.input_voltage),
            Register::SetVoltage => float(self.set_voltage),
            Register::SetCurrent => float(self.set_current),
            Register::Output => {
                let (v, i, p) = self.measurement();
                [float(v), float(i), float(p)].concat()
            }
            Register::Temperature => float(self.temperature),
            Register::PresetVoltage(slot) => float(self.presets[usize::from(slot.number() - 1)].0),
            Register::PresetCurrent(slot) => float(self.presets[usize::from(slot.number() - 1)].1),
            Register::Ovp => float(self.ovp),
            Register::Ocp => float(self.ocp),
            Register::Opp => float(self.opp),
            Register::Otp => float(self.otp),
            Register::Lvp => float(self.lvp),
            Register::Brightness => vec![self.brightness],
            Register::Volume => vec![self.volume],
            Register::FlagD8 | Register::FlagE1 => vec![1],
            Register::Capacity => float(self.capacity as f32),
            Register::Energy => float(self.energy as f32),
            Register::OutputState => vec![u8::from(self.output_on)],
            Register::Protection => vec![self.protection.code()],
            Register::Mode => vec![u8::from(!self.is_cc())],
            Register::Model => MODEL.to_vec(),
            Register::HardwareVersion => HARDWARE.to_vec(),
            Register::FirmwareVersion => FIRMWARE.to_vec(),
            Register::MaxVoltage => float(self.live_max_voltage()),
            Register::MaxCurrent => float(self.max_current),
            Register::OvpRange => float(self.ranges[0]),
            Register::OcpRange => float(self.ranges[1]),
            Register::OppRange => float(self.ranges[2]),
            Register::OtpRange => float(self.ranges[3]),
            Register::LvpRange => float(self.ranges[4]),
            Register::Dump => self.dump(),
        })
    }

    /// The 139-byte `GET FF` payload (PROTOCOL.md §6).
    fn dump(&self) -> Vec<u8> {
        let mut ids: Vec<u8> = vec![0xC0, 0xC1, 0xC2, 0xC3, 0xC4];
        ids.extend(0xC5..=0xD0);
        ids.extend(0xD1..=0xDA);
        ids.extend([0xDB, 0xDC, 0xDD, 0xE1, 0xE2, 0xE3]);
        ids.extend(0xE4..=0xE8);
        let mut dump = Vec::with_capacity(protocol::DUMP_LEN);
        for id in ids {
            let register = Register::from_id(id).expect("listed types are known");
            dump.extend(self.read(register).expect("listed types are readable"));
        }
        debug_assert_eq!(dump.len(), protocol::DUMP_LEN);
        dump
    }

    // ---- output model (SIM-08, SIM-10) ----------------------------------

    fn live_max_voltage(&self) -> f32 {
        self.live_max.unwrap_or_else(|| {
            (f64::from(self.input_voltage) - LIVE_MAX_OFFSET).clamp(0.0, 30.0) as f32
        })
    }

    /// Voltage the supply aims at when on, after the live-maximum clamp.
    fn target_voltage(&self) -> f64 {
        let ceiling = (f64::from(self.live_max_voltage()) - CLAMP_OFFSET).max(0.0);
        f64::from(self.set_voltage).clamp(0.0, ceiling)
    }

    fn is_cc(&self) -> bool {
        match self.load {
            Load::Open => false,
            Load::Ohms(ohms) => {
                self.output_on && f64::from(self.set_voltage) > f64::from(self.set_current) * ohms
            }
        }
    }

    fn measure(&self) -> (f64, f64, f64) {
        if self.level <= 0.0 {
            return (0.0, 0.0, 0.0);
        }
        let (v, i) = match self.load {
            Load::Open => (self.level * self.target_voltage(), 0.0),
            Load::Ohms(ohms) => {
                let cc = f64::from(self.set_voltage) > f64::from(self.set_current) * ohms;
                if cc {
                    let v = (self.level * f64::from(self.set_current) * ohms)
                        .min(self.target_voltage());
                    (v, self.level * f64::from(self.set_current))
                } else {
                    let v = self.level * self.target_voltage();
                    (v, v / ohms)
                }
            }
        };
        (v, i, v * i)
    }

    fn step_output(&mut self, dt: f64) {
        if self.output_on {
            self.level = (self.level + dt / RAMP_UP).min(1.0);
        } else {
            self.level = (self.level - dt / RAMP_DOWN).max(0.0);
        }
        let (v, i, p) = self.measure();
        if self.output_on {
            self.capacity += i * dt / 3600.0;
            self.energy += p * dt / 3600.0;
        }
        self.check_protections(v, i, p);
    }

    fn check_protections(&mut self, v: f64, i: f64, p: f64) {
        let vin = f64::from(self.input_voltage);
        let lvp = f64::from(self.lvp);
        // LVP is not latched (SIM-10).
        if self.protection == Protection::Lvp && vin >= lvp {
            self.protection = Protection::None;
        }
        if self.protection != Protection::None {
            return;
        }
        let tripped = if v > f64::from(self.ovp) {
            Protection::Ovp
        } else if i > f64::from(self.ocp) {
            Protection::Ocp
        } else if p > f64::from(self.opp) {
            Protection::Opp
        } else if f64::from(self.temperature) > f64::from(self.otp) {
            Protection::Otp
        } else if vin < lvp {
            Protection::Lvp
        } else {
            Protection::None
        };
        self.trip(tripped);
    }
}

fn float_of(data: &[u8]) -> f32 {
    f32::from_le_bytes([data[0], data[1], data[2], data[3]])
}
