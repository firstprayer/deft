use super::_common::{
    FFMPEG, VIDEO, atempo, ffmpeg, file_result, input_path, probe, stream, strs, temp_out,
    video_codec_args,
};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "video.speed",
    summary: "Speed up or slow down: --factor 2 or --factor 0.5",
    details: &["Sound keeps its pitch."],
    input: Input::each(VIDEO, "video"),
    output: Output::one(VIDEO, "video"),
    params: &[param(
        "factor",
        Kind::Float {
            min: 0.05,
            max: 100.0,
        },
        "2 is twice as fast, 0.5 half as fast",
    )
    .required()],
    engines: FFMPEG,
    examples: &["deft video speed clip.mp4 --factor 2 -o fast.mp4"],
    tags: &[
        "fast forward",
        "slow motion",
        "timelapse",
        "playback rate",
        "setpts",
    ],
    suffix: Some("speed"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let format = job.target(job.input().require_format()?);
    let out = temp_out(job, format);
    let factor = job.params.float("factor");
    let audio = stream(&probe(job, job.input(), OP.id)?, "audio").is_some();
    let mut args = strs(&["-i"]);
    args.push(input_path(job)?);
    args.extend(["-vf".to_string(), format!("setpts=PTS/{factor}")]);
    if audio {
        args.extend(["-af".to_string(), atempo(factor)]);
    }
    args.extend(video_codec_args(format, None));
    args.push(out.display().to_string());
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, format))
}
