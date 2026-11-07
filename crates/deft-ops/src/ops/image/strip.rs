use deft_core::prelude::*;

/// Formats whose metadata can be cut out without touching the picture. Others,
/// such as HEIC and TIFF, would need re-encoding, which changes the picture
/// and can lose pages or frames, so strip does not take them.
const CLEAN: &[Format] = &[Format::Jpeg, Format::Png, Format::Webp, Format::Gif];

pub static OP: Op = Op {
    id: "image.strip",
    summary: "Remove EXIF, GPS and text metadata",
    details: &[
        "JPEG, PNG, WebP and GIF only; the picture, every frame and the orientation stay as they are.",
        "Only the chunks that make the picture are kept, so metadata goes whatever it is called.",
        "Anything appended after the image, such as a motion photo's video, is removed.",
        "HEIC, AVIF and TIFF are refused: convert them to JPEG or PNG first.",
    ],
    input: Input::each(CLEAN, "JPEG, PNG, WebP or GIF"),
    output: Output::one(CLEAN, "image"),
    params: &[param(
        "drop-icc",
        Kind::Flag,
        "Also remove the color profile",
    )],
    examples: &["deft image strip photo-1.jpg -o clean.jpg"],
    tags: &[
        "metadata", "exif", "gps", "location", "privacy", "remove", "clean",
    ],
    result: &["bytes_before", "bytes_after"],
    suffix: Some("stripped"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let input = job.input();
    let format = input.require_format()?;
    let bytes = input.bytes()?;
    let before = bytes.len() as u64;
    let keep_icc = !job.params.flag("drop-icc");
    let mut warnings = Vec::new();
    let s = deft_tools_image::strip_metadata(bytes, format, keep_icc).ok_or_else(|| {
        err(
            ErrorCode::InputCorrupt,
            format!("{}: could not read the {format} structure", input.display),
        )
    })?;
    if s.trailing > 0 {
        warnings.push(format!(
            "removed {} appended after the image",
            format_bytes(s.trailing as u64)
        ));
    }
    let data = s.bytes;
    let after = data.len() as u64;
    let mut res = OpResult::one(Produced::bytes(data, format))
        .fact("bytes_before", before)
        .fact("bytes_after", after)
        .note(format!(
            "{} -> {}",
            format_bytes(before),
            format_bytes(after)
        ));
    for warning in warnings {
        res = res.warn(warning);
    }
    Ok(res)
}
