use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "subtitle",
    summary: "SRT/VTT: convert, shift, burn in, extract",
    aliases: &["subtitles", "srt", "vtt", "captions"],
    synonyms: &[
        ("add", "burn"),
        ("delay", "shift"),
        ("sync", "shift"),
        ("offset", "shift"),
        ("extract", "from-video"),
    ],
    order: 40,
};
