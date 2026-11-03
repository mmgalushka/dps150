//! Every value that depends on unobserved device behaviour, and every timing
//! rule, in one place (APP-10, APP.md §8). A hardware session changes one
//! line here and no logic.

use std::time::Duration;

/// Serial speed; 8N1.
pub const BAUD: u32 = 115_200;

/// A poll tick without a reply counts as missed; the link is lost after this
/// many in a row (LNK-01).
pub const MISSED_TICKS: u32 = 3;
/// No `FF` reply this long after the connect frame fails the handshake
/// (APP-01).
pub const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(3);
/// The connect frame is sent again once, this long after the first (APP-01).
pub const HANDSHAKE_RESEND: Duration = Duration::from_millis(1500);
/// Retry period while the link is down and Reconnect is on (LNK-06).
pub const RECONNECT_INTERVAL: Duration = Duration::from_secs(1);
/// Nothing is sent for this long after the connect frame (and, later, after
/// any SET), polling included (SAF-15, PROTOCOL.md §7.7).
pub const SET_GAP: Duration = Duration::from_millis(100);
/// Default poll rate in hertz (PRT-06).
pub const DEFAULT_REFRESH_HZ: u32 = 10;

/// Value written to `DB` to turn the output on / off [V 2026-11-02].
pub const OUTPUT_ON: u8 = 1;
pub const OUTPUT_OFF: u8 = 0;
