//! App-side policy constants (APP-10). The host policy for how far below a
//! limit a setpoint must stay is one named item here (SAFETY.md §1, LIM-04).

use std::time::Duration;

/// A setpoint must be at least this far below its limit: one resolution step.
pub const LIMIT_MARGIN_MV: u32 = 10;
pub const LIMIT_MARGIN_MA: u32 = 1;
pub const LIMIT_MARGIN_MW: u32 = 100;

/// A notice disappears after this long, or on the next key press (§5.12).
pub const NOTICE_DURATION: Duration = Duration::from_secs(3);

/// Longest typed entry (DESIGN.md §7.2).
pub const ENTRY_MAX_CHARS: usize = 5;
