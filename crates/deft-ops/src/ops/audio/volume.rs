use super::_common::{AUDIO, FFMPEG, filter_audio};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "audio.volume",
    summary: "Raise or lower the level: --gain 6 or --gain -3 (dB)",
    details: &["For a target loudness use deft audio normalize."],
    input: Input::each(AUDIO, "audio"),
    output: Output::one(AUDIO, "audio"),
    params: &[param(
        "gain",
        Kind::Float {
            min: -60.0,
            max: 60.0,
        },
        "Change in decibels",
    )
    .required()],
    engines: FFMPEG,
    examples: &["deft audio volume tone.mp3 --gain -6 -o quiet.mp3"],
    tags: &["louder", "quieter", "amplify", "decibel", "gain"],
    suffix: Some("volume"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    filter_audio(
        job,
        OP.id,
        &format!("volume={}dB", job.params.float("gain")),
    )
}
