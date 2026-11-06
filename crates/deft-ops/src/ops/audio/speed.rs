use super::_common::{AUDIO, FFMPEG, atempo, filter_audio};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "audio.speed",
    summary: "Change tempo, keeping pitch: --factor 1.5",
    details: &["--change-pitch plays it like a faster tape instead."],
    input: Input::each(AUDIO, "audio"),
    output: Output::one(AUDIO, "audio"),
    params: &[
        param(
            "factor",
            Kind::Float {
                min: 0.05,
                max: 100.0,
            },
            "2 is twice as fast, 0.5 half as fast",
        )
        .required(),
        param("change-pitch", Kind::Flag, "Let the pitch follow the speed"),
    ],
    engines: FFMPEG,
    examples: &["deft audio speed tone.mp3 --factor 2 -o fast.mp3"],
    tags: &[
        "tempo",
        "faster",
        "slower",
        "atempo",
        "nightcore",
        "playback rate",
    ],
    suffix: Some("speed"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let factor = p.float("factor");
    let filter = if p.flag("change-pitch") {
        format!("asetrate=44100*{factor},aresample=44100")
    } else {
        atempo(factor)
    };
    filter_audio(job, OP.id, &filter)
}
