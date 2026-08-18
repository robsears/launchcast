//! ARM/DISARM pending-confirmation state machine, plus a small scrolling
//! log of sent commands and their resolution. Port of `ground/code.py`'s
//! `tx_status`/`pending`/`CMD_CONFIRM_FRAMES` (~L364-494).
//!
//! Owned entirely by core0 (the only side that sends commands and sees
//! `link::LINK`/`radio::PACKET_COUNT` update), guarded by the same
//! cross-core `Mutex<CriticalSectionRawMutex, _>` pattern as `link::LINK`
//! -- core1's display task clones the (small, fixed-size) snapshot out
//! under the lock rather than holding it across a render.

use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
use embassy_sync::mutex::Mutex;
use heapless::String;
use launchcast_common as common;
use launchcast_ground_logic::LinkStatus;

/// Payload telemetry frames to see before declaring a pending ARM/DISARM
/// failed. Matches `code.py`'s `CMD_CONFIRM_FRAMES`.
const CMD_CONFIRM_FRAMES: u32 = 3;

/// Visible rows in the FLIGHT screen's command log panel.
pub const CMD_LOG_LINES: usize = 3;
pub const STATUS_LEN: usize = 32;

/// `u8`, not `common::State`/`common::Command` (both are plain associated-
/// const namespaces over `u8`, not enums -- see `common/src/lib.rs`), so
/// this is `Copy` for free and can be read out of the mutex by value.
#[derive(Clone, Copy)]
struct Pending {
    want: u8,
    packets_at_send: u32,
}

pub struct CmdLogState {
    /// Oldest first; a push rotates left and overwrites the last slot, so
    /// the most recent status is always `lines[CMD_LOG_LINES - 1]`.
    lines: [String<STATUS_LEN>; CMD_LOG_LINES],
    tx_status: String<STATUS_LEN>,
    pending: Option<Pending>,
}

pub static CMD_LOG: Mutex<CriticalSectionRawMutex, CmdLogState> = Mutex::new(CmdLogState {
    lines: [String::new(), String::new(), String::new()],
    tx_status: String::new(),
    pending: None,
});

/// Owned copy for the display task to render from -- cloned out from
/// behind the lock in one shot, same reasoning as `link::LinkState`.
#[derive(Clone)]
pub struct CmdLogSnapshot {
    pub lines: [String<STATUS_LEN>; CMD_LOG_LINES],
    pub tx_status: String<STATUS_LEN>,
}

impl CmdLogState {
    fn push_status(&mut self, s: &str) {
        self.tx_status.clear();
        let _ = self.tx_status.push_str(s);

        self.lines.rotate_left(1);
        let last = &mut self.lines[CMD_LOG_LINES - 1];
        last.clear();
        let _ = last.push_str(s);
    }
}

pub async fn snapshot() -> CmdLogSnapshot {
    let log = CMD_LOG.lock().await;
    CmdLogSnapshot {
        lines: log.lines.clone(),
        tx_status: log.tx_status.clone(),
    }
}

/// Record that a command was just sent. CHIRP resolves immediately (there's
/// no rocket state to confirm); ARM/DISARM opens a `pending` window that
/// [`poll`] resolves later. `packets_at_send` should be
/// `radio::PACKET_COUNT`'s value at send time -- matches `code.py`'s
/// `link.packets` snapshot in the same spot.
pub async fn record_send(cmd: u8, packets_at_send: u32) {
    let mut log = CMD_LOG.lock().await;
    if cmd == common::Command::CHIRP {
        log.push_status("SENT CHIRP");
    } else if cmd == common::Command::ARM {
        log.pending = Some(Pending {
            want: common::State::ARMED,
            packets_at_send,
        });
        log.push_status("SENT ARM...");
    } else if cmd == common::Command::DISARM {
        log.pending = Some(Pending {
            want: common::State::IDLE,
            packets_at_send,
        });
        log.push_status("SENT DISARM...");
    }
}

/// Resolve (or keep waiting on) a pending ARM/DISARM, given the rocket's
/// last-known state, the live packet counter, and the link's freshness
/// bucket. A no-op whenever nothing is pending. Matches `code.py`
/// ~L480-494 -- call this once per core0 loop iteration, not just when a
/// new frame arrives, so the link-lost branch can still fire on its own.
pub async fn poll(current_state: Option<u8>, packets_now: u32, status: LinkStatus) {
    let mut log = CMD_LOG.lock().await;
    let Some(pending) = log.pending else {
        return;
    };

    if current_state == Some(pending.want) {
        log.pending = None;
        log.push_status(if pending.want == common::State::ARMED {
            "ARMED OK"
        } else {
            "DISARMED OK"
        });
    } else if packets_now.wrapping_sub(pending.packets_at_send) >= CMD_CONFIRM_FRAMES {
        log.pending = None;
        log.push_status("CMD FAILED -- retry");
    } else if status == LinkStatus::Lost {
        // The branch above can't fire if no frames are arriving at all to
        // count -- don't leave a stale "...sent" status up forever if the
        // link itself has dropped.
        log.pending = None;
        log.push_status("CMD FAILED -- link lost");
    }
}
