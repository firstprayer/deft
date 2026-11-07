use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "audio",
    summary: "Convert, trim, normalize, denoise, cut silence (ffmpeg)",
    aliases: &["mp3", "sound", "music"],
    synonyms: &[
        ("loudness", "normalize"),
        ("silence", "remove-silence"),
        ("trim-silence", "remove-silence"),
    ],
    order: 35,
};
