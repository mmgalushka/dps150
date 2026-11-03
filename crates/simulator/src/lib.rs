//! A fake DPS-150 that speaks the real protocol (SIMULATOR.md).
//!
//! [`SimDevice`] is a pure state machine: no I/O and no clock of its own
//! (SIM-02). The host's bytes go in with [`SimDevice::feed`], time moves with
//! [`SimDevice::advance`], and what the device would send comes out of
//! [`SimDevice::take_output`]. [`Command`] is the fault-injection language of
//! SIMULATOR.md §5, shared by the library and the `dps150-sim` binary.
#![forbid(unsafe_code)]

mod control;
mod device;

pub use control::{Command, Outcome, ParseError};
pub use device::{Load, Protection, SET_DROP_WINDOW, SimDevice};
