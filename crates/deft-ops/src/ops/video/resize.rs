use super::_common::{
    FFMPEG, VIDEO, ffmpeg, file_result, input_path, scale_filter, strs, temp_out, video_codec_args,
};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "video.resize",
    summary: "Scale: --size 1280x, x720, 50%",
    input: Input::each(VIDEO, "video"),
    output: Output::one(VIDEO, "video"),
    params: &[param("size", Kind::Size, "Target size").required()],
    engines: FFMPEG,
    examples: &[
        "deft video resize clip.mp4 --size 160x -o small.mp4",
        "deft video resize clip.mp4 --size 50% -o half.mp4",
    ],
    tags: &["scale", "resolution", "480p", "720p", "1080p", "4k"],
    suffix: Some("resized"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let format = job.target(job.input().require_format()?);
    let out = temp_out(job, format);
    let mut args = strs(&["-i"]);
    args.push(input_path(job)?);
    args.extend(["-vf".to_string(), scale_filter(&job.params.size("size"))]);
    args.extend(video_codec_args(format, None));
    args.push(out.display().to_string());
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, format))
}
