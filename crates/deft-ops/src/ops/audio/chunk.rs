use super::_common::{AUDIO, FFMPEG, segments};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "audio.chunk",
    summary: "Cut into equal parts: --length 10:00",
    input: Input::each(AUDIO, "audio"),
    output: Output::many(AUDIO, "audio"),
    params: &[param("length", Kind::Duration, "Length of each part").required()],
    engines: FFMPEG,
    examples: &["deft audio chunk tone.mp3 --length 1 -o parts/"],
    tags: &[
        "split into parts",
        "segments",
        "pieces",
        "for transcription",
    ],
    result: &["files"],
    suffix: Some("chunks"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let length = job.params.seconds("length");
    if length <= 0.0 {
        return Err(bad_value("length", "must be above zero"));
    }
    let format = job.input().require_format()?;
    segments(job, OP.id, length, format, vec!["-vn".to_string()])
}
