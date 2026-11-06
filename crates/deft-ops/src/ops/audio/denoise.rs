use super::_common::{AUDIO, FFMPEG, filter_audio};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "audio.denoise",
    summary: "Reduce steady background noise such as hiss or hum",
    input: Input::each(AUDIO, "audio"),
    output: Output::one(AUDIO, "audio"),
    params: &[param(
        "level",
        Kind::Choice(&["light", "balanced", "strong"]),
        "How much to remove",
    )
    .default("balanced")],
    engines: FFMPEG,
    examples: &["deft audio denoise tone.mp3 --level light -o clean.mp3"],
    tags: &["noise reduction", "hiss", "hum", "clean up", "afftdn"],
    suffix: Some("denoised"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let reduction = match job.params.text("level").as_str() {
        "light" => 6,
        "strong" => 24,
        _ => 12,
    };
    filter_audio(job, OP.id, &format!("afftdn=nr={reduction}:nf=-40"))
}
