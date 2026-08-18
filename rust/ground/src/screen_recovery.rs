//! RECOVERY screen: distance/bearing/walking directions to the last known
//! rocket fix. Port of `screen_recovery.py`. The fix is latched (see
//! `link.rs`), so this keeps working after the rocket goes silent.

use embedded_graphics::draw_target::DrawTarget;
use embedded_graphics::pixelcolor::BinaryColor;
use heapless::String;
use launchcast_ground_logic::{bearing_deg, compass_point, haversine_m, relative_arrow, LinkStatus, Units};

use crate::display_util::text;
use crate::frame::Frame;

pub fn draw<D>(display: &mut D, frame: &Frame)
where
    D: DrawTarget<Color = BinaryColor>,
{
    let (Some(fix_lat), Some(fix_lon)) = (frame.fix_lat, frame.fix_lon) else {
        text(display, 4, 90, "NO FIX LATCHED", 2);
        text(display, 4, 120, "walk toward last seen bearing", 1);
        return;
    };

    let units = Units::default();

    let (Some(my_lat), Some(my_lon)) = (frame.my_lat, frame.my_lon) else {
        let mut line: String<24> = String::new();
        text(display, 4, 60, "ROCKET", 2);
        let _ = core::fmt::write(&mut line, format_args!("{fix_lat:.6}"));
        text(display, 4, 84, &line, 2);
        line.clear();
        let _ = core::fmt::write(&mut line, format_args!("{fix_lon:.6}"));
        text(display, 4, 108, &line, 2);
        text(display, 4, 140, "waiting for own GPS fix", 1);
        return;
    };

    let d = haversine_m(my_lat, my_lon, fix_lat, fix_lon);
    let b = bearing_deg(my_lat, my_lon, fix_lat, fix_lon);

    let mut line: String<24> = String::new();
    let _ = core::fmt::write(&mut line, format_args!("{:.0} {}", units.distance(d), units.distance_label()));
    text(display, 4, 52, &line, 3);

    line.clear();
    let _ = core::fmt::write(&mut line, format_args!("{:.0} {}", b, compass_point(b)));
    text(display, 180, 52, &line, 3);

    match relative_arrow(b, frame.my_heading) {
        Some(arrow) => text(display, 4, 100, arrow, 3),
        None => text(display, 4, 100, "walk to get heading", 2),
    }

    line.clear();
    let _ = core::fmt::write(&mut line, format_args!("rocket {fix_lat:.6}"));
    text(display, 4, 150, &line, 1);
    line.clear();
    let _ = core::fmt::write(&mut line, format_args!("       {fix_lon:.6}"));
    text(display, 4, 166, &line, 1);

    line.clear();
    let fix_age_s = frame.fix_age_ms.unwrap_or(0) as f32 / 1000.0;
    let _ = core::fmt::write(&mut line, format_args!("fix age {fix_age_s:.0}s"));
    text(display, 4, 190, &line, 1);

    if frame.status != LinkStatus::Live {
        text(display, 200, 190, "LATCHED -- rocket silent", 1);
    }
}
