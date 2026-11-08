use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "video",
    summary: "Fit into a frame, add text or a logo (ffmpeg)",
    aliases: &["mp4", "movie"],
    synonyms: &[
        ("letterbox", "pad"),
        ("vertical", "pad"),
        ("stamp", "watermark"),
        ("logo", "watermark"),
    ],
    order: 30,
};
