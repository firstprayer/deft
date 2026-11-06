use super::_common::{AUDIO, FFMPEG, ffmpeg, file_result, input_path, strs, temp_out};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "audio.strip",
    summary: "Remove tags and cover art; no re-encode",
    input: Input::each(AUDIO, "audio"),
    output: Output::one(AUDIO, "audio"),
    engines: FFMPEG,
    examples: &["deft audio strip tone.mp3 -o clean.mp3"],
    tags: &["metadata", "id3", "privacy", "remove tags"],
    suffix: Some("stripped"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let format = job.target(job.input().require_format()?);
    let out = temp_out(job, format);
    let mut args = strs(&["-i"]);
    args.push(input_path(job)?);
    args.extend(strs(&["-vn", "-map_metadata", "-1", "-c:a", "copy"]));
    args.push(out.display().to_string());
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, format))
}
