//! Shared by the level 1 tests: the pure link core piped to `SimDevice`
//! under one injected clock.
#![allow(dead_code)]

use std::time::Duration;

use device::{Event, Link, LinkConfig, State};
use protocol::{Dir, Frame, Reader, Register, command};
use simulator::SimDevice;

pub const MS: Duration = Duration::from_millis(1);

pub struct Rig {
    pub link: Link,
    pub sim: SimDevice,
    pub now: Duration,
    /// Every frame the link wrote, with the time it was written.
    pub written: Vec<(Duration, Frame)>,
    pub host: Reader,
    pub events: Vec<Event>,
}

impl Rig {
    pub fn new() -> Self {
        Self::with(LinkConfig::default())
    }

    pub fn with(config: LinkConfig) -> Self {
        Self {
            link: Link::new(config),
            sim: SimDevice::new(),
            now: Duration::ZERO,
            written: Vec::new(),
            host: Reader::new(Dir::HostToDevice),
            events: Vec::new(),
        }
    }

    /// The transport opens the port when the core asks.
    pub fn open_if_wanted(&mut self) {
        if self.link.wants_open(self.now) {
            self.link.port_opened(self.now);
        }
    }

    /// One millisecond: device output in, core poll, core output to the device.
    pub fn tick(&mut self) {
        self.now += MS;
        self.sim.advance(MS);
        let incoming = self.sim.take_output();
        self.link.feed(self.now, &incoming);
        self.open_if_wanted();
        let out = self.link.poll(self.now);
        if !out.is_empty() {
            for frame in self.host.feed(self.now, &out) {
                self.written.push((self.now, frame));
            }
            self.sim.feed(&out);
        }
        self.events.extend(self.link.take_events());
    }

    pub fn run(&mut self, span: Duration) {
        let end = self.now + span;
        while self.now < end {
            self.tick();
        }
    }

    pub fn run_until(&mut self, limit: Duration, done: impl Fn(&Rig) -> bool) {
        let end = self.now + limit;
        while self.now < end && !done(self) {
            self.tick();
        }
        assert!(done(self), "condition not reached within {limit:?}");
    }

    pub fn online() -> Self {
        let mut rig = Self::new();
        rig.run_until(Duration::from_secs(2), |rig| {
            rig.link.state() == State::Online
        });
        rig
    }

    pub fn polls(&self, since: Duration) -> usize {
        self.written
            .iter()
            .filter(|(at, frame)| {
                *at > since && frame.command() == command::DATA && frame.id() == Register::Dump.id()
            })
            .count()
    }
}

pub fn secs(seconds: u64) -> Duration {
    Duration::from_secs(seconds)
}
