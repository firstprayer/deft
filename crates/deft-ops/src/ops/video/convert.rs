use super::_common::{
    FFMPEG, VIDEO, ffmpeg, file_result, input_path, strs, temp_out, video_codec_args,
};
use deft_core::prelude::*;

const IN: &[Format] = &[
    Format::Mp4,
    Format::Webm,
    Format::Mov,
    Format::Mkv,
    Format::Avi,
    Format::Gif,
];

pub static OP: Op = Op {
    id: "video.convert",
    summary: "Change container and codec: --to mp4|webm|mov|mkv",
    input: Input::each(IN, "video|gif"),
    output: Output::one(VIDEO, "video"),
    params: &[
        param(
            "to",
            Kind::Choice(&["mp4", "webm", "mov", "mkv", "avi"]),
            "Target format (or give -o name.ext)",
        ),
        param(
            "fps",
            Kind::Float {
                min: 0.001,
                max: 120.0,
            },
            "Change the frame rate",
        ),
    ],
    converts: &[Edge {
        from: IN,
        to: VIDEO,
        quality: Quality::Faithful,
    }],
    engines: FFMPEG,
    examples: &[
        "deft video convert clip.mp4 --to webm -o clip.webm",
        "deft video convert clip.mp4 -o clip.mov",
        "deft video convert clip.mp4 --fps 5 -o slow-rate.mp4",
    ],
    tags: &[
        "format",
        "transcode",
        "mp4",
        "webm",
        "mov",
        "mkv",
        "frame rate",
        "gif to mp4",
    ],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let to = job.params.opt_text("to").ok_or_else(|| {
        err(
            ErrorCode::BadOption,
            "--to is required (or give -o name.ext)",
        )
    })?;
    let format = Format::from_name(&to).expect("choice");
    let out = temp_out(job, format);
    let mut args = strs(&["-i"]);
    args.push(input_path(job)?);
    // Odd GIF sizes cannot be encoded as yuv420p.
    let mut filters = vec!["scale=trunc(iw/2)*2:trunc(ih/2)*2".to_string()];
    if let Some(fps) = job.params.opt_float("fps") {
        filters.push(format!("fps={fps}"));
    }
    args.extend(["-vf".to_string(), filters.join(",")]);
    args.extend(video_codec_args(format, None));
    args.push(out.display().to_string());
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, format))
}
