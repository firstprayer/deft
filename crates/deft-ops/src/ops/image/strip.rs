use super::_common::{IN, INOUT, encode, open, profile};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "image.strip",
    summary: "Remove EXIF, GPS and text metadata",
    details: &[
        "JPEG, PNG and WebP stay pixel-identical. Orientation is kept so images stay upright.",
        "Anything appended after a JPEG, such as a motion photo's video, is removed.",
    ],
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
    let keep_icc = !job.params.flag("drop-icc");
    let mut warnings = Vec::new();
    let (data, format) = match deft_tools_image::strip_metadata(bytes, format, keep_icc) {
        Some(s) => {
            if s.trailing > 0 {
                warnings.push(format!(
                    "removed {} appended after the image",
                    format_bytes(s.trailing as u64)
                ));
            }
            (s.bytes, format)
        }
        None => {
            // Other formats: re-encoding drops metadata.
            let (img, f) = open(job)?;
            let f = if deft_tools_image::WRITABLE.contains(&f) {
                f
            } else {
                Format::Png
            };
            let icc = profile(job).filter(|_| keep_icc);
            let opt = deft_tools_image::Encode {
                icc: icc.as_deref(),
                ..Default::default()
            };
            let (data, notes) = encode(job, &img, f, opt)?;
            warnings.extend(notes);
            (data, f)
        }
    };
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
