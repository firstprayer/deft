use super::_common::IN;
use deft_core::prelude::*;
use deft_tools_image::exif::Fields;

const OUT: &[Format] = &[Format::Jpeg, Format::Png];

pub static OP: Op = Op {
    id: "image.meta",
    summary: "Read EXIF, or set --title --author --copyright --date",
    details: &[
        "With no options it prints every tag, with GPS as decimal degrees.",
        "With options it writes a copy; pixels are untouched. Remove tags with image strip.",
    ],
    input: Input::each(IN, "image"),
    output: Output::one(OUT, "image"),
    params: &[
        param("title", Kind::Text, "Description"),
        param("author", Kind::Text, "Artist"),
        param("copyright", Kind::Text, "Copyright notice"),
        param("comment", Kind::Text, "User comment"),
        param("date", Kind::Text, "When taken: 2026-05-01 12:30:00"),
    ],
    examples: &["deft image meta photo-1.jpg --author Ada --copyright 'CC BY' -o signed.jpg"],
    tags: &[
        "exif",
        "gps",
        "camera",
        "date taken",
        "metadata",
        "location",
    ],
    result: &["tags"],
    suffix: Some("meta"),
    run,
    ..Op::DEFAULT
};

/// EXIF wants `YYYY:MM:DD HH:MM:SS`.
fn exif_date(text: &str) -> Result<String> {
    let bad = || bad_value("date", "use YYYY-MM-DD or YYYY-MM-DD HH:MM:SS");
    let text = text.trim().replace('T', " ");
    let (date, time) = text.split_once(' ').unwrap_or((&text, "00:00:00"));
    let date = date.replace(['-', '/'], ":");
    let parts: Vec<&str> = date.split(':').collect();
    let digits = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_digit());
    if parts.len() != 3 || !digits(parts[0], 4) || !digits(parts[1], 2) || !digits(parts[2], 2) {
        return Err(bad());
    }
    let clock: Vec<&str> = time.split(':').collect();
    if clock.len() != 3 || !clock.iter().all(|c| digits(c, 2)) {
        return Err(bad());
    }
    Ok(format!("{date} {time}"))
}

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let input = job.input();
    let format = input.require_format()?;
    let fields = Fields {
        title: p.opt_text("title"),
        author: p.opt_text("author"),
        copyright: p.opt_text("copyright"),
        comment: p.opt_text("comment"),
        date: p.opt_text("date").map(|d| exif_date(&d)).transpose()?,
    };
    let writing = fields.title.is_some()
        || fields.author.is_some()
        || fields.copyright.is_some()
        || fields.comment.is_some()
        || fields.date.is_some();
    if !writing {
        let tags = deft_tools_image::exif::read(input.bytes()?);
        let n = tags.len();
        return Ok(OpResult::report(tags, format!("{n} tags")));
    }
    let target = job.target(format);
    if target != format {
        return Err(err(
            ErrorCode::BadOption,
            "metadata keeps the format; convert the image first",
        ));
    }
    let bytes = deft_tools_image::exif::write(input.bytes()?, format, &fields)?;
    Ok(OpResult::one(Produced::bytes(bytes, format)))
}
