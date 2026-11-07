use super::_common::{FUZZ, IN, INOUT, QUALITY, open, save};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "image.trim",
    summary: "Cut away plain borders; --speck 4 ignores stray dots",
    input: Input::each(IN, "image"),
    output: Output::one(INOUT, "image"),
    details: &[
        "--speck is the largest patch of touching pixels to treat as dust, so scans trim cleanly.",
        "Too high also drops small real marks, such as a full stop or a page number.",
    ],
    params: &[
        FUZZ.default("5"),
        param(
            "speck",
            Kind::Int {
                min: 0,
                max: 1_000_000,
            },
            "Ignore patches up to this many pixels",
        )
        .default("0"),
        QUALITY,
    ],
    examples: &[
        "deft image trim bordered.png -o tight.png",
        "deft image trim bordered.png --speck 4 -o clean.png",
    ],
    tags: &["autocrop", "remove whitespace", "border", "margins"],
    result: &["width", "height"],
    suffix: Some("trimmed"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let (img, format) = open(job)?;
    let fuzz = job.params.float("fuzz") as f32;
    let speck = job.params.int("speck") as usize;
    match deft_tools_image::edit::trim_box(&img, fuzz, speck) {
        Some((x, y, w, h)) => save(job, &img.crop_imm(x, y, w, h), format),
        None => Ok(save(job, &img, format)?.warn("the image is one colour; nothing to trim")),
    }
}
