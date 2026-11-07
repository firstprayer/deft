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
    let info = probe(job, job.input(), OP.id)?;
    let audio = stream(&info, "audio").is_some();
    // Without this a timelapse keeps every frame, at a rate players refuse.
    let rate = stream(&info, "video")
        .and_then(|v| v["avg_frame_rate"].as_str())
        .filter(|r| !r.starts_with('0'))
        .map_or(String::new(), |r| format!(",fps={r}"));
    let mut args = strs(&["-i"]);
    args.push(input_path(job)?);
    args.extend(["-vf".to_string(), format!("setpts=PTS/{factor}{rate}")]);
    if audio {
        args.extend(["-af".to_string(), atempo(factor)]);
    }
    args.extend(video_codec_args(format, None));
    args.push(out.display().to_string());
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, format))
}
