use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "gps",
    summary: "GPX and KML tracks: convert, distance and time",
    aliases: &["gpx", "kml", "track"],
    synonyms: &[("stats", "info")],
    order: 59,
};
