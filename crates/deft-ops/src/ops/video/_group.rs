use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "video",
    summary: "Stabilize, fit into a frame, add text or a logo (ffmpeg)",
    aliases: &["mp4", "movie"],
    synonyms: &[
        ("deshake", "stabilize"),
        ("steady", "stabilize"),
        ("letterbox", "pad"),
        ("vertical", "pad"),
        ("stamp", "watermark"),
        ("logo", "watermark"),
    ],
    order: 30,
};
