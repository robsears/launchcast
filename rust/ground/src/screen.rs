//! Which screen is currently showing (`code.py`'s `screen`/`SCREEN_*`
//! constants). Lives on core1 (owns both buttons and the display), read
//! by `button_task` (to decide whether an ARM/DISARM hold means "send
//! the command" or "go back a screen") and `display_task` (to pick which
//! screen to render). A single small atomic, not a `Mutex` -- both
//! readers/writers are on the same core/executor already, and there's
//! nothing here that needs a critical section beyond what a plain atomic
//! store/load already gives.

use portable_atomic::{AtomicU8, Ordering};

pub const FLIGHT: u8 = 0;
pub const RECOVERY: u8 = 1;
pub const DIAG: u8 = 2;
pub const COUNT: u8 = 3;
const NAMES: [&str; COUNT as usize] = ["FLIGHT", "RECOVERY", "DIAG"];

static CURRENT: AtomicU8 = AtomicU8::new(FLIGHT);

pub fn current() -> u8 {
    CURRENT.load(Ordering::Relaxed)
}

pub fn name(index: u8) -> &'static str {
    NAMES[(index % COUNT) as usize]
}

pub fn current_name() -> &'static str {
    name(current())
}

pub fn next_name() -> &'static str {
    name((current() + 1) % COUNT)
}

pub fn prev_name() -> &'static str {
    name((current() + COUNT - 1) % COUNT)
}

/// MENU: always advances, regardless of which screen is showing --
/// matches `code.py`'s `screen = (screen + 1) % SCREEN_COUNT`.
pub fn advance() {
    CURRENT.store((current() + 1) % COUNT, Ordering::Relaxed);
}

/// ARM/DISARM-as-BACK, off FLIGHT -- matches `code.py`'s `screen =
/// (screen - 1) % SCREEN_COUNT`.
pub fn back() {
    CURRENT.store((current() + COUNT - 1) % COUNT, Ordering::Relaxed);
}
