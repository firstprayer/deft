use super::_common::{IN, load};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "gps.info",
    summary: "Distance, time, climb and point counts",
    input: Input::each(IN, "gpx|kml"),
    output: Output::report(),
    examples: &["deft gps info ride.gpx"],
    tags: &["how far", "route length", "elevation", "strava", "garmin"],
    result: &[
        "tracks",
        "places",
        "points",
        "distance_km",
        "seconds",
        "climb_m",
    ],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let geo = load(job.input())?;
    let s = deft_tools_data::geo::stats(&geo);
    let km = (s.distance_km * 100.0).round() / 100.0;
    let note = format!("{km} km, {} points", s.points);
    Ok(OpResult::report(
        facts![
            ("tracks", geo.tracks.len()),
            ("places", geo.places.len()),
            ("points", s.points),
            ("distance_km", km),
            ("seconds", s.seconds),
            ("climb_m", s.climb_m.round())
        ],
        note,
    ))
}
