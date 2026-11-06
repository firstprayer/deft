use super::_common::{FFMPEG, VIDEO, ffmpeg, file_result, input_path, strs, temp_out};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "video.mute",
    summary: "Remove the audio track",
    input: Input::each(VIDEO, "video"),
    output: Output::one(VIDEO, "video"),
    engines: FFMPEG,
    examples: &["deft video mute clip.mp4 -o silent.mp4"],
    tags: &["silent", "remove audio", "no sound"],
    suffix: Some("muted"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let format = job.target(job.input().require_format()?);
    let out = temp_out(job, format);
    let mut args = strs(&["-i"]);
    args.push(input_path(job)?);
    args.extend(strs(&["-c:v", "copy", "-an"]));
    args.push(out.display().to_string());
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, format))
}
