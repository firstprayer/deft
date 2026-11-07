use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "video",
    summary: "Trim, merge, convert, compress, subtitles, GIF, frames (ffmpeg)",
    aliases: &["mp4", "movie"],
    synonyms: &[
        ("thumbnail", "frames"),
        ("poster", "frames"),
        ("gif", "to-gif"),
        ("screenshot", "frames"),
        ("slideshow", "from-images"),
        ("timelapse", "speed"),
        ("slow", "speed"),
        ("slowmo", "speed"),
        ("letterbox", "pad"),
        ("music", "add-audio"),
        ("soundtrack", "add-audio"),
        ("segment", "chunk"),
        ("burn", "add-subtitles"),
        ("subtitle", "add-subtitles"),
        ("subtitles", "add-subtitles"),
        ("stamp", "watermark"),
        ("logo", "watermark"),
    ],
    order: 30,
};
