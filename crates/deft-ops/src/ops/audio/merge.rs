use super::_common::{AUDIO, FFMPEG, concat_args, ffmpeg, file_result, temp_out};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "audio.merge",
    summary: "Join audio files in order",
    input: Input::all(AUDIO, 2, "audio"),
    output: Output::one(&[Format::Mp3], "mp3"),
    engines: FFMPEG,
    examples: &["deft audio merge tone.mp3 tone2.mp3 -o both.mp3"],
    tags: &["concat", "join", "combine", "append"],
    suffix: Some("merged"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let out = temp_out(job, Format::Mp3);
    let mut args = concat_args(job, OP.id, false)?;
    args.push(out.display().to_string());
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, Format::Mp3))
}
