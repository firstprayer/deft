use super::_common::{AUDIO, FFMPEG, ffmpeg, file_result, input_path, strs, temp_out};
use deft_core::prelude::*;

const FIELDS: &[(&str, &str)] = &[
    ("title", "title"),
    ("artist", "artist"),
    ("album", "album"),
    ("genre", "genre"),
    ("year", "date"),
];

pub static OP: Op = Op {
    id: "audio.tag",
    summary: "Set tags: --title --artist --album --year, cover --image",
    details: &["Other tags stay. The sound is copied, not re-encoded. Read tags with audio info."],
    input: Input::each(AUDIO, "audio"),
    output: Output::one(AUDIO, "audio"),
    params: &[
        param("title", Kind::Text, "Title"),
        param("artist", Kind::Text, "Artist"),
        param("album", Kind::Text, "Album"),
        param("genre", Kind::Text, "Genre"),
        param("year", Kind::Text, "Year"),
        param("track", Kind::Int { min: 1, max: 9999 }, "Track number"),
        param("image", Kind::Path, "Cover art (MP3, M4A and FLAC)"),
    ],
    engines: FFMPEG,
    examples: &["deft audio tag tone.mp3 --title Hello --artist Ada --year 2026 -o tagged.mp3"],
    tags: &["id3", "metadata", "album art", "cover art", "edit tags"],
    suffix: Some("tagged"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let format = job.target(job.input().require_format()?);
    let out = temp_out(job, format);
    let mut args = strs(&["-i"]);
    args.push(input_path(job)?);
    let cover = p.opt_path("image");
    if let Some(c) = &cover {
        if !matches!(format, Format::Mp3 | Format::M4a | Format::Flac) {
            return Err(bad_value(
                "image",
                format!("{format} cannot hold cover art"),
            ));
        }
        if !c.exists() {
            return Err(bad_value("image", format!("{} not found", c.display())));
        }
        args.extend(strs(&["-i"]));
        args.push(c.display().to_string());
        args.extend(strs(&[
            "-map",
            "0:a",
            "-map",
            "1:0",
            "-disposition:v:0",
            "attached_pic",
        ]));
    }
    let mut changed = cover.is_some();
    for (option, key) in FIELDS {
        if let Some(v) = p.opt_text(option) {
            args.extend(["-metadata".to_string(), format!("{key}={v}")]);
            changed = true;
        }
    }
    if let Some(n) = p.opt_int("track") {
        args.extend(["-metadata".to_string(), format!("track={n}")]);
        changed = true;
    }
    if !changed {
        return Err(err(ErrorCode::BadOption, "give at least one tag to set"));
    }
    args.extend(strs(&["-c", "copy"]));
    if format == Format::Mp3 {
        args.extend(strs(&["-id3v2_version", "3"]));
    }
    args.push(out.display().to_string());
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, format))
}
