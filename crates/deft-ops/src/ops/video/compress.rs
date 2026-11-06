use super::_common::{FFMPEG, VIDEO, duration, ffmpeg, input_path, probe, stream, strs, temp_out};
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
    tags: &[
        "shrink",
        "reduce size",
        "smaller",
        "email",
        "h264",
        "too big",
        "large",
    ],
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
    let scale = p
        .opt_int("max-height")
        .map(|h| vec!["-vf".to_string(), format!("scale=-2:'min({h},ih)'")])
        .unwrap_or_default();
    let mut video = strs(&[
        "-c:v", "libx264", "-preset", "medium", "-pix_fmt", "yuv420p",
    ]);
    video.extend(scale);
    match p.opt_bytes("target-size") {
        Some(target) => to_size(job, &video, target, &out)?,
        None => {
            let crf = match p.text("level").as_str() {
                "light" => "23",
                "strong" => "32",
                _ => "28",
            };
            let mut args = strs(&["-i"]);
            args.push(input_path(job)?);
            args.extend(video);
            args.extend(strs(&["-crf", crf, "-c:a", "aac", "-b:a", "96k"]));
            args.extend(strs(&["-movflags", "+faststart"]));
            args.push(out.display().to_string());
            ffmpeg(job, OP.id, args)?;
        }
    }
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

/// Audio bitrate when aiming for a file size, in kbps.
const AUDIO_KBPS: f64 = 64.0;
/// Two-pass encoding lands close to the target but can pass it, so this many
/// attempts may be needed.
const ATTEMPTS: usize = 3;

/// Spends the whole size budget: two passes put the bits where the picture needs
/// them, and a silent video gives nothing to audio.
fn to_size(job: &Job, video: &[String], target: u64, out: &std::path::Path) -> Result<()> {
    let info = probe(job, job.input(), OP.id)?;
    let d = duration(&info).max(0.1);
    let has_audio = stream(&info, "audio").is_some();
    let audio_kbps = if has_audio { AUDIO_KBPS } else { 0.0 };
    let mut video_kbps = target as f64 * 8.0 / 1000.0 / d * 0.95 - audio_kbps;
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
    let log = job.ctx.temp_path("x264").display().to_string();
    let src = input_path(job)?;
    for _ in 0..ATTEMPTS {
        let pass = |n: &str| {
            let mut args = strs(&["-i"]);
            args.push(src.clone());
            args.extend(video.iter().cloned());
            args.extend(["-b:v".to_string(), format!("{video_kbps:.0}k")]);
            args.extend(["-pass".to_string(), n.to_string()]);
            args.extend(["-passlogfile".to_string(), log.clone()]);
            args
        };
        let mut first = pass("1");
        first.extend(strs(&["-an", "-f", "null", "-"]));
        ffmpeg(job, OP.id, first)?;
        let mut second = pass("2");
        if has_audio {
            second.extend(strs(&["-c:a", "aac", "-b:a", "64k"]));
        }
        second.extend(strs(&["-movflags", "+faststart"]));
        second.push(out.display().to_string());
        ffmpeg(job, OP.id, second)?;
        let size = std::fs::metadata(out)?.len();
        if size <= target {
            return Ok(());
        }
        video_kbps *= target as f64 / size as f64 * 0.97;
    }
    Ok(())
}
