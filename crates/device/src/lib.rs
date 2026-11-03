//! The device layer: a pure link state machine ([`Link`]) and the thread that
//! moves bytes between it and a serial port ([`Device`]). APP.md §5 to §7.
#![forbid(unsafe_code)]

pub mod constants;
mod link;
mod transport;

pub use link::{Cause, Event, Link, LinkConfig, LinkStats, OpenError, Refusal, Snapshot, State};
pub use protocol::Write;
pub use transport::{Command, Device, Message, StartupError};
