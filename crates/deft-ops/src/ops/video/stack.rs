use super::_common::{
    FFMPEG, VIDEO, ffmpeg, file_result, probe, stream, strs, temp_out, video_codec_args, video_size,
};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "video.stack",
    summary: "Play side by side: --direction",
    details: &["Clips are scaled to match the first. Sound comes from the first."],
    input: Input::all(VIDEO, 2, "video"),
    output: Output::one(&[Format::Mp4], "mp4"),
    params: &[param(
        "direction",
        Kind::Choice(&["horizontal", "vertical"]),
        "Layout",
    )
    .default("horizontal")],
    engines: FFMPEG,
    examples: &["deft video stack clip.mp4 clip2.mp4 -o compare.mp4"],
    tags: &[
        "side by side",
        "split screen",
        "hstack",
        "vstack",
        "comparison",
    ],
    suffix: Some("stacked"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let vertical = job.params.text("direction") == "vertical";
    let info = probe(job, job.input(), OP.id)?;
    let (w, h) =
        video_size(&info).ok_or_else(|| err(ErrorCode::InputUnsupported, "no video stream"))?;
    let mut args = Vec::new();
    let mut filter = String::new();
    let mut labels = String::new();
    for (i, input) in job.inputs.iter().enumerate() {
        args.extend(strs(&["-i"]));
        args.push(input.local_path(job.ctx)?.display().to_string());
        let scale = if vertical {
            format!("scale={w}:-2")
        } else {
            format!("scale=-2:{h}")
        };
        filter.push_str(&format!("[{i}:v]{scale},setsar=1[v{i}];"));
        labels.push_str(&format!("[v{i}]"));
    }
    let n = job.inputs.len();
    let stack = if vertical { "vstack" } else { "hstack" };
    filter.push_str(&format!("{labels}{stack}=inputs={n}[v]"));
    args.extend(["-filter_complex".to_string(), filter]);
    args.extend(strs(&["-map", "[v]"]));
    if stream(&info, "audio").is_some() {
        args.extend(strs(&["-map", "0:a"]));
    }
    let out = temp_out(job, Format::Mp4);
    args.extend(video_codec_args(Format::Mp4, None));
    args.push(out.display().to_string());
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, Format::Mp4))
}
