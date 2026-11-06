use super::_common::{
    AUDIO, FFMPEG, audio_out, end_args, ffmpeg, file_result, input_path, seek_args, temp_out,
};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "audio.split",
    summary: "Cut out a part: --from 0:10 --to-time 0:25",
    input: Input::each(AUDIO, "audio"),
    output: Output::one(AUDIO, "audio"),
    params: &[
        param("from", Kind::Duration, "Start time").default("0"),
        param(
            "to-time",
            Kind::Duration,
            "End time (default: end of the audio)",
        ),
    ],
    engines: FFMPEG,
    examples: &[
        "deft audio split tone.mp3 --from 0:00 --to-time 0:01 -o part.mp3",
        "deft audio trim tone.mp3 --from 1 -o tail.mp3",
    ],
    tags: &["trim", "cut", "clip", "excerpt"],
    suffix: Some("clip"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let format = job.target(job.input().require_format()?);
    let out = temp_out(job, format);
    let mut args = seek_args(job.params);
    args.extend(end_args(job.params)?);
    args.push("-i".to_string());
    args.push(input_path(job)?);
    args.extend(audio_out(job, OP.id, format, &out)?);
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, format))
}
