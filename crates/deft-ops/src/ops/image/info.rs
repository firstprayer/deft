use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "image.info",
    summary: "Format, dimensions, EXIF orientation, GPS",
    input: Input::each(
        &[
            Format::Png,
            Format::Jpeg,
            Format::Webp,
            Format::Gif,
            Format::Tiff,
            Format::Bmp,
            Format::Ico,
            Format::Avif,
            Format::Heic,
            Format::Svg,
        ],
        "image",
    ),
    output: Output::report(),
    examples: &["deft image info photo-1.jpg"],
    tags: &["dimensions", "resolution", "metadata", "exif", "size"],
    result: &["width", "height", "orientation", "has_metadata", "has_gps"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let input = job.input();
    let bytes = input.bytes()?;
    let format = input.require_format()?;
    let heif = deft_tools_image::heif::read(bytes);
    let dims = if format == Format::Svg {
        let img = deft_tools_image::render_svg(bytes, None)?;
        Some((img.width(), img.height()))
    } else if let Some(h) = &heif {
        Some((h.width, h.height))
    } else {
        deft_tools_image::dimensions(bytes)
    };
    if dims.is_none() && !matches!(format, Format::Heic | Format::Avif) {
        return Err(err(
            ErrorCode::InputCorrupt,
            format!("{}: not a readable image", input.display),
        ));
    }
    let (metadata, gps) = deft_tools_image::metadata_summary(bytes, format);
    let orientation = match &heif {
        Some(h) => h.orientation,
        None => deft_tools_image::exif_orientation(bytes),
    };
    // Report the size as displayed: orientations 5-8 swap width and height.
    let dims = dims.map(|(w, h)| if orientation >= 5 { (h, w) } else { (w, h) });
    let note = match dims {
        Some((w, h)) => format!("{w}x{h}"),
        None => "dimensions unknown".to_string(),
    };
    let facts = facts![
        ("width", dims.map(|d| d.0)),
        ("height", dims.map(|d| d.1)),
        ("orientation", orientation),
        ("has_metadata", metadata),
        ("has_gps", gps),
    ];
    Ok(OpResult::report(facts, note))
}
