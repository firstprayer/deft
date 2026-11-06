use super::_common::{
    FFMPEG, VIDEO, end_args, ffmpeg, file_result, input_path, seek_args, strs, temp_out,
    video_codec_args,
};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "video.split",
    summary: "Cut out a part: --from 0:10 --to-time 0:25",
    details: &["--fast copies streams without re-encoding; cuts land on keyframes."],
    input: Input::each(VIDEO, "video"),
    output: Output::one(VIDEO, "video"),
    params: &[
        param("from", Kind::Duration, "Start time").default("0"),
        param(
            "to-time",
            Kind::Duration,
            "End time (default: end of the video)",
        ),
        param("fast", Kind::Flag, "Stream copy, no re-encoding"),
    ],
    engines: FFMPEG,
    examples: &[
        "deft video split clip.mp4 --from 0:01 --to-time 0:02 -o part.mp4",
        "deft video trim clip.mp4 --from 1 --fast -o tail.mp4",
    ],
    tags: &[
        "trim", "cut", "clip", "excerpt", "shorten", "keep", "middle", "clip", "part", "trim",
        "shorten",
    ],
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
    if job.params.flag("fast") {
        args.extend(strs(&["-c", "copy"]));
    } else {
        args.extend(video_codec_args(format, None));
    }
    args.push(out.display().to_string());
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, format))
}
