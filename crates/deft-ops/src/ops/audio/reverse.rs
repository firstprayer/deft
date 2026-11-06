use super::_common::{AUDIO, FFMPEG, filter_audio};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "audio.reverse",
    summary: "Play backwards",
    input: Input::each(AUDIO, "audio"),
    output: Output::one(AUDIO, "audio"),
    engines: FFMPEG,
    examples: &["deft audio reverse tone.mp3 -o backwards.mp3"],
    tags: &["backwards", "areverse"],
    suffix: Some("reversed"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    filter_audio(job, OP.id, "areverse")
}
