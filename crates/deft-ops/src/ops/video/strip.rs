use super::_common::{FFMPEG, VIDEO, ffmpeg, file_result, input_path, strs, temp_out};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "video.strip",
    summary: "Remove metadata; no re-encode",
    input: Input::each(VIDEO, "video"),
    output: Output::one(VIDEO, "video"),
    engines: FFMPEG,
    examples: &["deft video strip clip.mp4 -o clean.mp4"],
    tags: &["metadata", "gps", "privacy", "exif"],
    suffix: Some("stripped"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let format = job.target(job.input().require_format()?);
    let out = temp_out(job, format);
    let mut args = strs(&["-i"]);
    args.push(input_path(job)?);
    args.extend(strs(&[
        "-map",
        "0",
        "-map",
        "-0:d?",
        "-map_metadata",
        "-1",
        "-map_chapters",
        "-1",
        "-c",
        "copy",
    ]));
    args.push(out.display().to_string());
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, format))
}
