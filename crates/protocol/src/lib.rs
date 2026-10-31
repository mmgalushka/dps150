//! Frame codec, checksum and typed registers. No I/O.
//!
//! Layers, bottom up: [`Frame`] is one checked frame on the wire; [`Reader`]
//! turns a byte stream into frames; [`Register`] names the parameter types of
//! PROTOCOL.md §4; [`Reply`] and [`Dump`] give a device frame typed values.
//! Setpoints and limits are integer thousandths ([`value`]), so a negative or
//! non-finite one cannot be built (PRT-04, LIM-01).
//!
//! SET frames are not built here yet: the write path arrives with the setpoint
//! milestone, after the read path is proven (PLAN.md §1).
#![forbid(unsafe_code)]

mod dump;
mod frame;
mod reader;
mod register;
mod reply;
pub mod value;

pub use dump::{DUMP_LEN, Dump, Mode, OutputReading, Preset};
pub use frame::{Dir, Frame, FrameError, checksum, command};
pub use reader::{Reader, STALE_FRAME, Stats};
pub use register::{Register, Slot};
pub use reply::{DecodeError, Reply};
pub use value::{MilliCelsius, Milliamps, Millivolts, Milliwatts, ValueError};
