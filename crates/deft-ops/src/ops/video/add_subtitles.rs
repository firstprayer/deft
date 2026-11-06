use super::_common::{
    FFMPEG, VIDEO, duration, ffmpeg, file_result, input_path, probe, stream, strs, temp_out,
    video_codec_args,
};
use deft_core::prelude::*;
use deft_tools_image::image::{DynamicImage, RgbaImage};
use std::fmt::Write as _;

pub static OP: Op = Op {
    id: "video.add-subtitles",
    summary: "Add subtitles: burned into the picture, or --soft",
    details: &[
        "Burned subtitles use this machine's fonts, so Chinese and other scripts work.",
        "--soft adds a track the viewer can turn off (no re-encoding).",
    ],
    input: Input::each(VIDEO, "video"),
    output: Output::one(VIDEO, "video"),
    params: &[
        param("subtitles", Kind::Path, "An .srt or .vtt file").required(),
        param(
            "soft",
            Kind::Flag,
            "Add a selectable track instead of burning in",
        ),
        param(
            "font-size",
            Kind::Float {
                min: 1.0,
                max: 1000.0,
            },
            "Text size in pixels (default: 5% of the height)",
        ),
        param("color", Kind::Color, "Text color").default("white"),
        param("position", Kind::Position, "bottom or top").default("bottom"),
        param("font", Kind::Text, "Font family"),
    ],
    mounts: &["subtitle burn"],
    engines: FFMPEG,
    examples: &[
        "deft video add-subtitles clip.mp4 --subtitles talk.srt -o subbed.mp4",
        "deft subtitle burn clip.mp4 --subtitles zh.srt -o zh.mp4",
        "deft video add-subtitles clip.mp4 --subtitles talk.vtt --soft -o soft.mp4",
    ],
    tags: &[
        "burn subtitles",
        "hardsub",
        "captions",
        "srt",
        "add subtitles to video",
        "open captions",
    ],
    result: &["cues"],
    suffix: Some("subtitled"),
    run,
    ..Op::DEFAULT
};

fn soft(job: &Job, format: Format, cues: &[deft_tools_text::Cue]) -> Result<OpResult> {
    let srt = job.ctx.temp_path("subs.srt");
    std::fs::write(&srt, deft_tools_text::cues_to_srt(cues))?;
    let codec = match format {
        Format::Mp4 | Format::Mov => "mov_text",
        Format::Webm => "webvtt",
        Format::Mkv => "srt",
        other => {
            return Err(err(
                ErrorCode::InputUnsupported,
                format!("{other} files cannot hold a subtitle track"),
            )
            .hint("use mp4, mov, mkv or webm, or burn them in"));
        }
    };
    let out = temp_out(job, format);
    let mut args = strs(&["-i"]);
    args.push(input_path(job)?);
    args.extend(["-i".to_string(), srt.display().to_string()]);
    args.extend(strs(&[
        "-map", "0", "-map", "1", "-c", "copy", "-c:s", codec,
    ]));
    args.push(out.display().to_string());
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, format)
        .fact("cues", cues.len())
        .note(format!("{} lines, soft track", cues.len())))
}

/// Draw each cue on a transparent band, then overlay the bands as one timed
/// image stream. This needs no libass and works for any script.
fn burn(job: &Job, format: Format, cues: &[deft_tools_text::Cue]) -> Result<OpResult> {
    let p = job.params;
    let info = probe(job, job.input(), OP.id)?;
    let v = stream(&info, "video")
        .ok_or_else(|| err(ErrorCode::InputUnsupported, "no video stream"))?;
    let (w, h) = (
        v["width"].as_u64().unwrap_or(1280) as u32,
        v["height"].as_u64().unwrap_or(720) as u32,
    );
    let size = p
        .opt_float("font-size")
        .unwrap_or(h as f64 * 0.05)
        .max(10.0) as f32;
    let style = deft_tools_image::TextStyle {
        outline: Some(([0, 0, 0, 255], size * 0.12)),
        font: p.opt_text("font"),
        ..deft_tools_image::TextStyle::new(size, p.color("color"))
    };
    let max_em = w as f32 * 0.9 / size;
    let mut drawn = Vec::new();
    for cue in cues {
        let text = deft_tools_image::wrap_text(&cue.text, max_em).join("\n");
        drawn.push(deft_tools_image::render_text(&text, &style)?);
    }
    let band = drawn.iter().map(|d| d.height()).max().unwrap_or(1) + 4;
    let dir = job.ctx.temp_path("bands");
    std::fs::create_dir_all(&dir)?;
    let write_band = |name: &str, top: Option<&RgbaImage>| -> Result<std::path::PathBuf> {
        let mut canvas = RgbaImage::new(w, band);
        if let Some(t) = top {
            let x = (w as i64 - t.width() as i64) / 2;
            deft_tools_image::image::imageops::overlay(
                &mut canvas,
                t,
                x,
                band as i64 - t.height() as i64 - 2,
            );
        }
        let path = dir.join(name);
        std::fs::write(
            &path,
            deft_tools_image::encode(
                &DynamicImage::ImageRgba8(canvas),
                Format::Png,
                Default::default(),
            )?,
        )?;
        Ok(path)
    };
    let blank = write_band("blank.png", None)?;
    let mut list = String::from("ffconcat version 1.0\n");
    let mut clock = 0.0;
    for (i, (cue, img)) in cues.iter().zip(&drawn).enumerate() {
        if cue.start > clock {
            let _ = writeln!(
                list,
                "file '{}'\nduration {:.3}",
                blank.display(),
                cue.start - clock
            );
        }
        let path = write_band(&format!("cue-{i}.png"), Some(img))?;
        let _ = writeln!(
            list,
            "file '{}'\nduration {:.3}",
            path.display(),
            (cue.end - cue.start.max(clock)).max(0.04)
        );
        clock = cue.end.max(clock);
    }
    let total = duration(&info).max(clock) + 1.0;
    let _ = writeln!(
        list,
        "file '{}'\nduration {:.3}\nfile '{}'",
        blank.display(),
        total - clock,
        blank.display()
    );
    let list_path = dir.join("list.ffconcat");
    std::fs::write(&list_path, list)?;
    let margin = (h as f64 * 0.05) as u32;
    let y = if p.position("position").y < 0.5 {
        margin.to_string()
    } else {
        format!("H-h-{margin}")
    };
    let out = temp_out(job, format);
    let mut args = strs(&["-i"]);
    args.push(input_path(job)?);
    args.extend(strs(&["-f", "concat", "-safe", "0", "-i"]));
    args.push(list_path.display().to_string());
    args.extend([
        "-filter_complex".to_string(),
        format!("[1:v]format=rgba[s];[0:v][s]overlay=0:{y}:eof_action=pass[v]"),
    ]);
    args.extend(strs(&["-map", "[v]", "-map", "0:a?", "-shortest"]));
    args.extend(video_codec_args(format, None));
    args.push(out.display().to_string());
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, format)
        .fact("cues", cues.len())
        .note(format!("{} lines burned in", cues.len())))
}

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let path = p.path("subtitles");
    let raw = std::fs::read(&path).map_err(|e| {
        err(
            ErrorCode::InputNotFound,
            format!("--subtitles {}: {e}", path.display()),
        )
    })?;
    let cues = deft_tools_text::parse_subtitles(&String::from_utf8_lossy(&raw));
    if cues.is_empty() {
        return Err(err(
            ErrorCode::InputCorrupt,
            format!("--subtitles {}: no cues found", path.display()),
        )
        .hint("expected an .srt or .vtt file"));
    }
    let format = job.target(job.input().require_format()?);
    if p.flag("soft") {
        soft(job, format, &cues)
    } else {
        burn(job, format, &cues)
    }
}
