use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "audio",
    summary: "Convert, trim, merge, fade, speed, tag, normalize (ffmpeg)",
    aliases: &["mp3", "sound", "music"],
    synonyms: &[
        ("loudness", "normalize"),
        ("gain", "volume"),
        ("louder", "volume"),
        ("quieter", "volume"),
        ("silence", "remove-silence"),
        ("trim-silence", "remove-silence"),
        ("tempo", "speed"),
        ("metadata", "tag"),
        ("tags", "tag"),
        ("id3", "tag"),
        ("overlay", "mix"),
        ("segment", "chunk"),
    ],
    order: 35,
};
