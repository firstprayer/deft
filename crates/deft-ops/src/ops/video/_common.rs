//! Helpers shared by the video operations.

use deft_core::prelude::*;
use serde_json::Value as Json;
use std::path::PathBuf;

pub const VIDEO: &[Format] = &[
    Format::Mp4,
    Format::Webm,
    Format::Mov,
    Format::Mkv,
    Format::Avi,
];
pub const FFMPEG: &[&str] = &["ffmpeg", "ffprobe"];

/// Run ffmpeg with `args`; stdin is closed and the banner is suppressed.
pub fn ffmpeg(job: &Job, id: &str, args: Vec<String>) -> Result<()> {
    let path = deft_engines::require(job.ctx, "ffmpeg", id)?;
    let mut all: Vec<String> = ["-hide_banner", "-nostdin", "-loglevel", "error", "-y"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    all.extend(args);
    deft_engines::run(job.ctx, &path, &all)?;
    Ok(())
}

/// Stream and format metadata from ffprobe.
pub fn probe(job: &Job, input: &InputFile, id: &str) -> Result<Json> {
    let path = deft_engines::require(job.ctx, "ffprobe", id)?;
    let file = input.local_path(job.ctx)?;
    let args = [
        "-v",
        "error",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
    ]
    .iter()
    .map(|s| s.to_string())
    .chain([file.display().to_string()])
    .collect::<Vec<_>>();
    let out = deft_engines::run(job.ctx, &path, &args)?;
    serde_json::from_slice(&out.stdout)
        .map_err(|e| err(ErrorCode::EngineFailed, format!("ffprobe output: {e}")))
}

pub fn duration(info: &Json) -> f64 {
    info["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0.0)
}

pub fn stream<'a>(info: &'a Json, kind: &str) -> Option<&'a Json> {
    info["streams"]
        .as_array()?
        .iter()
        .find(|s| s["codec_type"] == kind)
}

/// Encoder flags for a target container.
pub fn video_codec_args(format: Format, crf: Option<u32>) -> Vec<String> {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    match format {
        Format::Webm => {
            let mut a = s(&[
                "-c:v",
                "libvpx-vp9",
                "-b:v",
                "0",
                "-row-mt",
                "1",
                "-c:a",
                "libopus",
            ]);
            a.extend(["-crf".to_string(), crf.unwrap_or(33).to_string()]);
            a
        }
        _ => {
            let mut a = s(&[
                "-c:v", "libx264", "-preset", "veryfast", "-pix_fmt", "yuv420p", "-c:a", "aac",
                "-b:a", "128k",
            ]);
            a.extend(["-crf".to_string(), crf.unwrap_or(23).to_string()]);
            if format == Format::Mp4 || format == Format::Mov {
                a.extend(["-movflags".to_string(), "+faststart".to_string()]);
            }
            a
        }
    }
}

pub fn temp_out(job: &Job, format: Format) -> PathBuf {
    job.ctx.temp_path(&format!("out.{}", format.ext()))
}

pub fn input_path(job: &Job) -> Result<String> {
    Ok(job.input().local_path(job.ctx)?.display().to_string())
}

pub fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// `-ss` before the input: seeks instantly and stays frame-accurate when
/// re-encoding.
pub fn seek_args(p: &Params) -> Vec<String> {
    match p.opt_seconds("from") {
        Some(f) if f > 0.0 => vec!["-ss".to_string(), format!("{f}")],
        _ => vec![],
    }
}

/// `-t` before the input, so reading stops there. Filters such as palettegen
/// otherwise wait for the end of the whole file.
pub fn end_args(p: &Params) -> Result<Vec<String>> {
    let Some(end) = p.opt_seconds("to-time") else {
        return Ok(vec![]);
    };
    let start = p.opt_seconds("from").unwrap_or(0.0);
    if end <= start {
        return Err(bad_value("to-time", "must be after --from"));
    }
    Ok(vec!["-t".to_string(), format!("{}", end - start)])
}

/// Refuse a `--from` at or past the end, which ffmpeg turns into an empty or
/// wrong file without an error.
pub fn check_from(p: &Params, length: f64) -> Result<()> {
    match p.opt_seconds("from") {
        Some(from) if length > 0.0 && from >= length => Err(bad_value(
            "from",
            format!("{from}s is past the end ({})", format_duration(length)),
        )),
        _ => Ok(()),
    }
}

/// ffmpeg scale expression for a size spec, keeping dimensions even.
pub fn scale_filter(s: &SizeSpec) -> String {
    if let Some(p) = s.percent {
        return format!("scale=trunc(iw*{0}/200)*2:trunc(ih*{0}/200)*2", p);
    }
    match (s.width, s.height) {
        (Some(w), Some(h)) => {
            format!("scale={w}:{h}:force_original_aspect_ratio=decrease:force_divisible_by=2")
        }
        (Some(w), None) => format!("scale={w}:-2"),
        (None, Some(h)) => format!("scale=-2:{h}"),
        (None, None) => "null".into(),
    }
}

/// Display size of the first video stream, taking rotation metadata into
/// account, because ffmpeg's filters see the rotated picture.
pub fn video_size(info: &Json) -> Option<(u32, u32)> {
    let v = stream(info, "video")?;
    let (w, h) = (v["width"].as_u64()? as u32, v["height"].as_u64()? as u32);
    Some(if (rotation(info).abs() as i64) % 180 == 90 {
        (h, w)
    } else {
        (w, h)
    })
}

/// Degrees a player turns the first video stream, from its rotation metadata.
pub fn rotation(info: &Json) -> f64 {
    stream(info, "video")
        .and_then(|v| v["side_data_list"].as_array())
        .and_then(|l| l.iter().find_map(|d| d["rotation"].as_f64()))
        .unwrap_or(0.0)
}

/// The `atempo` chain for a speed factor. One stage only spans 0.5 to 2.
pub fn atempo(factor: f64) -> String {
    let mut left = factor;
    let mut stages = Vec::new();
    while left > 2.0 {
        stages.push("atempo=2".to_string());
        left /= 2.0;
    }
    while left < 0.5 {
        stages.push("atempo=0.5".to_string());
        left /= 0.5;
    }
    stages.push(format!("atempo={left}"));
    stages.join(",")
}

/// ffmpeg colour syntax for an RGBA colour.
pub fn ffmpeg_color(c: [u8; 4]) -> String {
    format!("0x{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// Video re-encoded through a filter, with the audio passed through.
pub fn filter_video(job: &Job, id: &str, filter: &str) -> Result<OpResult> {
    let format = job.target(job.input().require_format()?);
    let out = temp_out(job, format);
    let mut args = strs(&["-i"]);
    args.push(input_path(job)?);
    args.extend(["-vf".to_string(), filter.to_string()]);
    args.extend(video_codec_args(format, None));
    args.push(out.display().to_string());
    ffmpeg(job, id, args)?;
    Ok(file_result(out, format))
}

/// Cut one file into pieces of `length` seconds with ffmpeg's segment muxer.
pub fn segments(
    job: &Job,
    id: &str,
    length: f64,
    format: Format,
    codec: Vec<String>,
) -> Result<OpResult> {
    let dir = job.ctx.temp_path("chunks");
    std::fs::create_dir_all(&dir)?;
    let stem = job.input().stem();
    let pattern = dir.join(format!("{stem}-%03d.{}", format.ext()));
    let mut args = strs(&["-i"]);
    args.push(input_path(job)?);
    args.extend(codec);
    args.extend(strs(&["-f", "segment", "-reset_timestamps", "1"]));
    args.extend(["-segment_time".to_string(), format!("{length}")]);
    args.push(pattern.display().to_string());
    ffmpeg(job, id, args)?;
    let mut files: Vec<_> = std::fs::read_dir(&dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .collect();
    files.sort();
    let n = files.len();
    let outputs = files
        .into_iter()
        .map(|f| {
            let name = f
                .file_name()
                .map(|x| x.to_string_lossy().to_string())
                .unwrap_or_default();
            Produced::file(f, Some(format)).named(name)
        })
        .collect();
    Ok(OpResult::many(outputs)
        .fact("files", n)
        .note(format!("{n} files")))
}

pub fn file_result(path: PathBuf, format: Format) -> OpResult {
    OpResult::one(Produced::file(path, Some(format)))
}
