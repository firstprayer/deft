use super::_common::{FFMPEG, VIDEO, duration, ffmpeg, input_path, probe, scale_filter, strs};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "video.thumbnail",
    summary: "One frame as an image, or --at a time",
    input: Input::each(VIDEO, "video"),
    output: Output::one(&[Format::Jpeg, Format::Png, Format::Webp], "image"),
    params: &[
        param(
            "at",
            Kind::Times,
            "Time of the frame (default: chosen by ffmpeg)",
        ),
        param("size", Kind::Size, "Scale, e.g. 320x or 50%"),
    ],
    engines: FFMPEG,
    examples: &[
        "deft video thumbnail clip.mp4 -o cover.jpg",
        "deft video thumbnail clip.mp4 --at 0:01 --size 160x -o small.png",
    ],
    tags: &["poster", "preview image", "cover", "still"],
    suffix: Some("thumbnail"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let format = job.target(Format::Jpeg);
    let out = job.ctx.temp_path(&format!("thumb.{}", format.ext()));
    let mut args = Vec::new();
    let mut filters = Vec::new();
    match p.opt_times("at") {
        Some(times) => {
            let t = *times.first().unwrap_or(&0.0);
            let length = duration(&probe(job, job.input(), OP.id)?);
            if t >= length {
                return Err(bad_value(
                    "at",
                    format!("{t}s is past the end ({})", format_duration(length)),
                ));
            }
            args.extend(["-ss".to_string(), format!("{t}")]);
        }
        None => filters.push("thumbnail=100".to_string()),
    }
    if let Some(size) = p.opt_size("size") {
        filters.push(scale_filter(&size));
    }
    args.extend(strs(&["-i"]));
    args.push(input_path(job)?);
    if !filters.is_empty() {
        args.extend(["-vf".to_string(), filters.join(",")]);
    }
    args.extend(strs(&["-frames:v", "1", "-update", "1"]));
    args.push(out.display().to_string());
    ffmpeg(job, OP.id, args)?;
    Ok(OpResult::one(Produced::file(out, Some(format))))
}
