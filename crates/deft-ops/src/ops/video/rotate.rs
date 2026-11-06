use super::_common::{
    FFMPEG, VIDEO, ffmpeg, file_result, input_path, strs, temp_out, video_codec_args,
};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "video.rotate",
    summary: "Turn clockwise: --angle 90",
    input: Input::each(VIDEO, "video"),
    output: Output::one(VIDEO, "video"),
    params: &[param(
        "angle",
        Kind::Float {
            min: -360.0,
            max: 360.0,
        },
        "Degrees clockwise: 90, 180, 270, -90",
    )
    .default("90")],
    engines: FFMPEG,
    examples: &["deft video rotate clip.mp4 --angle 180 -o turned.mp4"],
    tags: &["turn", "orientation", "portrait", "landscape"],
    suffix: Some("rotated"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let format = job.target(job.input().require_format()?);
    let out = temp_out(job, format);
    let filter = match right_angle(job.params.float("angle"))? {
        0 => "null",
        180 => "hflip,vflip",
        270 => "transpose=2",
        _ => "transpose=1",
    };
    let mut args = strs(&["-i"]);
    args.push(input_path(job)?);
    args.extend(["-vf".to_string(), filter.to_string()]);
    args.extend(video_codec_args(format, None));
    args.push(out.display().to_string());
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, format))
}
