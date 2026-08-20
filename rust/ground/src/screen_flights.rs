//! FLIGHTS: browse the flights the rocket currently has stored (see
//! `rocket-logic::flight_summary`) and select one to fetch on SUMMARY.
//! Ordinal list only ("FLIGHT 1".."FLIGHT N") -- no per-row durations/
//! stats, just the count; a background prefetch (`flight_index.rs`) is
//! quietly caching every flight's summary regardless, shown here as a
//! "FETCHING LOGS: X/Y..." banner while it's still in progress.
//! Selecting a flight (CHIRP) either jumps straight to a cached summary
//! (often already prefetched by the time you pick it) or sends a fresh
//! `GET_SUMMARY_BASE` request (see `main.rs`'s button-forwarding
//! handler).
//!
//! Rendered from `flight_index.rs`'s cache state, not a live telemetry
//! byte -- see that module's docs on why: a live `flight_count` can't
//! tell "the rocket really has N flights" apart from "the rocket
//! power-cycled and lost its RAM-only flights since we last heard from
//! it."

use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::BinaryColor;
use heapless::String;
use launchcast_ground_logic::LinkStatus;

use crate::display_util::text;
use crate::flight_index::IndexState;
use crate::frame::Frame;

const ROW0_Y: i32 = 56;
const ROW_STEP: i32 = 22;
/// How many rows fit without needing scroll logic -- generous relative
/// to `common::MAX_STORED_FLIGHTS` (32) for now; revisit with a
/// scrolling window if that ever turns out to matter in practice.
const MAX_VISIBLE_ROWS: u8 = 7;

pub fn draw<D>(display: &mut D, frame: &Frame)
where
    D: DrawTarget<Color = BinaryColor>,
{
    match frame.flight_index_state {
        IndexState::Idle if frame.status == LinkStatus::Lost => {
            text(display, 4, 70, "FLIGHT LOGS UNAVAILABLE", 1);
            text(display, 4, 94, "rocket not found", 1);
        }
        IndexState::Idle | IndexState::Pending { .. } => {
            text(display, 4, 70, "LOADING FLIGHT LIST...", 1);
        }
        IndexState::Failed => {
            text(display, 4, 70, "FAILED TO FETCH DATA", 1);
            text(display, 4, 94, "from rocket -- try again", 1);
        }
        IndexState::Empty => {
            text(display, 4, 70, "THERE ARE NO LOGS", 1);
            text(display, 4, 94, "on the rocket", 1);
        }
        IndexState::Ready { count } => draw_list(display, frame, count),
    }
}

fn draw_list<D>(display: &mut D, frame: &Frame, count: u8)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let selected = frame.selected_flight;
    let visible = count.min(MAX_VISIBLE_ROWS);
    let mut line: String<24> = String::new();

    // Background per-flight prefetch progress (see flight_index.rs) --
    // doesn't block browsing/selecting, just an FYI banner in the gap
    // between the header and the first list row.
    if let Some((cached, total)) = frame.prefetch_progress {
        line.clear();
        let _ = core::fmt::write(&mut line, format_args!("FETCHING LOGS: {cached}/{total}..."));
        text(display, 4, 44, &line, 1);
    }

    for i in 0..visible {
        line.clear();
        let marker = if i == selected { '>' } else { ' ' };
        let _ = core::fmt::write(&mut line, format_args!("{marker}FLIGHT {}", i + 1));
        text(display, 4, ROW0_Y + i as i32 * ROW_STEP, &line, 2);
    }
    if count > MAX_VISIBLE_ROWS {
        line.clear();
        let _ = core::fmt::write(&mut line, format_args!("...and {} more", count - MAX_VISIBLE_ROWS));
        text(display, 4, ROW0_Y + MAX_VISIBLE_ROWS as i32 * ROW_STEP, &line, 1);
    }
}
