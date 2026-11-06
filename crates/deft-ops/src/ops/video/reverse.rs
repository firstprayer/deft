use super::_common::{
    FFMPEG, VIDEO, ffmpeg, file_result, input_path, probe, stream, strs, temp_out, video_codec_args,
};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "video.reverse",
    summary: "Play backwards, picture and sound",
    details: &["Holds every frame in memory; trim long videos first."],
    input: Input::each(VIDEO, "video"),
    output: Output::one(VIDEO, "video"),
    engines: FFMPEG,
    examples: &["deft video reverse clip.mp4 -o backwards.mp4"],
    tags: &["rewind", "backwards", "boomerang"],
    suffix: Some("reversed"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let format = job.target(job.input().require_format()?);
    let out = temp_out(job, format);
    let audio = stream(&probe(job, job.input(), OP.id)?, "audio").is_some();
    let mut args = strs(&["-i"]);
    args.push(input_path(job)?);
    args.extend(strs(&["-vf", "reverse"]));
    if audio {
        args.extend(strs(&["-af", "areverse"]));
    }
    args.extend(video_codec_args(format, None));
    args.push(out.display().to_string());
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, format))
}
