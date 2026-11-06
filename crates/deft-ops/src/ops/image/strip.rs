use super::_common::{IN, INOUT, open};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "image.strip",
    summary: "Remove EXIF, GPS and text metadata",
    details: &["JPEG and PNG stay pixel-identical. Orientation is kept so images stay upright."],
    input: Input::each(IN, "image"),
    output: Output::one(INOUT, "image"),
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
    let (data, format) =
        match deft_tools_image::strip_metadata(bytes, format, !job.params.flag("drop-icc")) {
            Some(d) => (d, format),
            None => {
                // Other formats: re-encoding drops metadata.
                let (img, f) = open(job)?;
                let f = if deft_tools_image::WRITABLE.contains(&f) {
                    f
                } else {
                    Format::Png
                };
                (deft_tools_image::encode(&img, f, Default::default())?, f)
            }
        };
    let after = data.len() as u64;
    Ok(OpResult::one(Produced::bytes(data, format))
        .fact("bytes_before", before)
        .fact("bytes_after", after)
        .note(format!(
            "{} -> {}",
            format_bytes(before),
            format_bytes(after)
        )))
}
