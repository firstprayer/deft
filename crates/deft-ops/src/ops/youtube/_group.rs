use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "youtube",
    summary: "Info, download, audio, subtitles, thumbnail (yt-dlp)",
    aliases: &["yt"],
    synonyms: &[("video", "download"), ("mp3", "audio")],
    order: 70,
};
