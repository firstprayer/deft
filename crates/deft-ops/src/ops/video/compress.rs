use super::_common::{FFMPEG, VIDEO, duration, ffmpeg, input_path, probe, strs, temp_out};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "video.compress",
    summary: "Smaller file: --level, --target-size, --max-height",
    input: Input::each(VIDEO, "video"),
    output: Output::one(&[Format::Mp4], "mp4"),
    params: &[
        param(
            "level",
            Kind::Choice(&["light", "balanced", "strong"]),
            "How hard to compress",
        )
        .default("balanced"),
        param("target-size", Kind::Bytes, "Aim for this file size"),
        param(
            "max-height",
            Kind::Int { min: 16, max: 8640 },
            "Scale down to this height, e.g. 720",
        ),
    ],
    engines: FFMPEG,
    examples: &[
        "deft video compress clip.mp4 --level strong -o small.mp4",
        "deft video compress clip.mp4 --target-size 500KB --max-height 240 -o tiny.mp4",
    ],
    tags: &["shrink", "reduce size", "smaller", "email", "h264"],
    result: &["bytes_before", "bytes_after", "ratio"],
    errors: &[ErrorCode::TargetUnreachable],
    suffix: Some("compressed"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let input = job.input();
    let out = temp_out(job, Format::Mp4);
    let mut args = strs(&["-i"]);
    args.push(input_path(job)?);
    args.extend(strs(&[
        "-c:v",
        "libx264",
        "-preset",
        "medium",
        "-pix_fmt",
        "yuv420p",
        "-c:a",
        "aac",
        "-movflags",
        "+faststart",
    ]));
    if let Some(h) = p.opt_int("max-height") {
        args.extend(["-vf".to_string(), format!("scale=-2:'min({h},ih)'")]);
    }
    match p.opt_bytes("target-size") {
        Some(target) => {
            let d = duration(&probe(job, input, OP.id)?).max(0.1);
            let audio_kbps = 64.0;
            let total_kbps = target as f64 * 8.0 / 1000.0 / d * 0.95;
            let video_kbps = total_kbps - audio_kbps;
            if video_kbps < 30.0 {
                return Err(err(
                    ErrorCode::TargetUnreachable,
                    format!(
                        "{} is too small for a {} video",
                        format_bytes(target),
                        format_duration(d)
                    ),
                )
                .hint("try a larger --target-size or trim it first"));
            }
            args.extend([
                "-b:v".to_string(),
                format!("{video_kbps:.0}k"),
                "-maxrate".to_string(),
                format!("{:.0}k", video_kbps * 1.3),
                "-bufsize".to_string(),
                format!("{:.0}k", video_kbps * 2.0),
                "-b:a".to_string(),
                "64k".to_string(),
            ]);
        }
        None => {
            let crf = match p.text("level").as_str() {
                "light" => "23",
                "strong" => "32",
                _ => "28",
            };
            args.extend([
                "-crf".to_string(),
                crf.to_string(),
                "-b:a".to_string(),
                "96k".to_string(),
            ]);
        }
    }
    args.push(out.display().to_string());
    ffmpeg(job, OP.id, args)?;
    let before = input.bytes()?.len() as u64;
    let after = std::fs::metadata(&out)?.len();
    let ratio = after as f64 / before.max(1) as f64;
    let mut res = OpResult::one(Produced::file(out, Some(Format::Mp4)))
        .fact("bytes_before", before)
        .fact("bytes_after", after)
        .fact("ratio", (ratio * 1000.0).round() / 1000.0)
        .note(format!(
            "{} -> {} ({:+.0}%)",
            format_bytes(before),
            format_bytes(after),
            (ratio - 1.0) * 100.0
        ));
    if after >= before {
        res = res.warn("the output is not smaller; the input is already well compressed");
    }
    Ok(res)
}
