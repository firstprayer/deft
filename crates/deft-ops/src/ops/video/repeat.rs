use super::_common::{FFMPEG, VIDEO, ffmpeg, file_result, input_path, strs, temp_out};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "video.repeat",
    summary: "Repeat a video: --count 3 plays it three times",
    input: Input::each(VIDEO, "video"),
    output: Output::one(VIDEO, "video"),
    params: &[param("count", Kind::Int { min: 2, max: 1000 }, "Total plays").default("2")],
    engines: FFMPEG,
    examples: &["deft video repeat clip.mp4 --count 3 -o thrice.mp4"],
    tags: &["loop", "stream_loop", "again"],
    suffix: Some("repeated"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let format = job.target(job.input().require_format()?);
    let out = temp_out(job, format);
    let mut args = vec![
        "-stream_loop".to_string(),
        (job.params.int("count") - 1).to_string(),
        "-i".to_string(),
        input_path(job)?,
    ];
    args.extend(strs(&["-c", "copy"]));
    args.push(out.display().to_string());
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, format))
}
