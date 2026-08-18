//! DIAG screen: raw counters and link stats, for debugging on the bench.
//! Port of `screen_diagnostics.py`.

use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::BinaryColor;
use heapless::String;
use launchcast_common::{self as common, Sensor};

use crate::display_util::text;
use crate::frame::Frame;

/// Join an iterator of names with spaces into a fixed-capacity string,
/// matching Python's `" ".join(...)`. Silently truncates (via `push_str`
/// failing) rather than panicking if it ever runs long -- there are only
/// six possible sensor names, so in practice this never gets close.
fn join_names<const N: usize>(names: impl Iterator<Item = &'static str>) -> String<N> {
    let mut out: String<N> = String::new();
    for (i, name) in names.enumerate() {
        if i > 0 {
            let _ = out.push(' ');
        }
        let _ = out.push_str(name);
    }
    out
}

pub fn draw<D>(display: &mut D, frame: &Frame, tel: &common::Telemetry)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let mut line: String<48> = String::new();

    let _ = core::fmt::write(&mut line, format_args!("pkts {}  rej {}", frame.packets, frame.rejects));
    text(display, 4, 52, &line, 1);

    line.clear();
    let _ = core::fmt::write(
        &mut line,
        format_args!("rssi {}  snr {}", frame.rssi.unwrap_or(0), frame.snr.unwrap_or(0)),
    );
    text(display, 4, 72, &line, 1);

    line.clear();
    let _ = core::fmt::write(&mut line, format_args!("state {}", tel.state_name()));
    text(display, 4, 92, &line, 1);

    line.clear();
    let _ = core::fmt::write(&mut line, format_args!("uptime {:.1}s", tel.uptime_ms as f32 / 1000.0));
    text(display, 4, 112, &line, 1);

    line.clear();
    let _ = core::fmt::write(&mut line, format_args!("counter {}", tel.counter));
    text(display, 4, 132, &line, 1);

    let up: String<48> = join_names(Sensor::present(tel.sensors));
    line.clear();
    let _ = core::fmt::write(&mut line, format_args!("up: {up}"));
    text(display, 4, 152, &line, 1);

    let mut down: String<48> = join_names(Sensor::missing(tel.sensors));
    if down.is_empty() {
        let _ = down.push_str("none");
    }
    line.clear();
    let _ = core::fmt::write(&mut line, format_args!("down: {down}"));
    text(display, 4, 168, &line, 1);

    line.clear();
    let _ = core::fmt::write(
        &mut line,
        format_args!("accel {:.2} {:.2} {:.2}", tel.accel_g[0], tel.accel_g[1], tel.accel_g[2]),
    );
    text(display, 4, 188, &line, 1);

    text(display, 4, 204, frame.tx_status, 1);
}
