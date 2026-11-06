use super::_common::{
    FFMPEG, VIDEO, end_args, ffmpeg, file_result, input_path, scale_filter, seek_args, temp_out,
};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "video.to-gif",
    summary: "Animated GIF: --fps, --size, --from, --to-time",
    input: Input::each(VIDEO, "video"),
    output: Output::one(&[Format::Gif], "gif"),
    params: &[
        param(
            "fps",
            Kind::Float {
                min: 0.1,
                max: 50.0,
            },
            "Frames per second",
        )
        .default("12"),
        param(
            "size",
            Kind::Size,
            "Output size (default: at most 480 wide)",
        ),
        param("from", Kind::Duration, "Start time").default("0"),
        param(
            "to-time",
            Kind::Duration,
            "End time (default: end of the video)",
        ),
    ],
    converts: &[Edge {
        from: VIDEO,
        to: &[Format::Gif],
        quality: Quality::Approximate,
    }],
    engines: FFMPEG,
    examples: &[
        "deft video to-gif clip.mp4 -o clip.gif",
        "deft video to-gif clip.mp4 --from 0:01 --to-time 0:02 --size 240x --fps 8 -o short.gif",
    ],
    tags: &["animation", "meme", "loop"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let out = temp_out(job, Format::Gif);
    // Without --size, large videos shrink but small ones are never enlarged.
    let scale = p
        .opt_size("size")
        .map_or("scale='min(480,iw)':-2".to_string(), |s| scale_filter(&s));
    let filter = format!(
        "fps={},{scale}:flags=lanczos,split[a][b];[a]palettegen[p];[b][p]paletteuse",
        p.float("fps")
    );
    let mut args = seek_args(p);
    args.extend(end_args(p)?);
    args.push("-i".to_string());
    args.push(input_path(job)?);
    args.extend(["-filter_complex".to_string(), filter]);
    args.push(out.display().to_string());
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, Format::Gif))
}
