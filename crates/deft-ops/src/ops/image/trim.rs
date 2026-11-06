use super::_common::{FUZZ, IN, INOUT, QUALITY, open, save};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "image.trim",
    summary: "Cut away plain borders, matching the corner colour",
    input: Input::each(IN, "image"),
    output: Output::one(INOUT, "image"),
    params: &[FUZZ.default("5"), QUALITY],
    examples: &["deft image trim bordered.png -o tight.png"],
    tags: &["autocrop", "remove whitespace", "border", "margins"],
    result: &["width", "height"],
    suffix: Some("trimmed"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let (img, format) = open(job)?;
    let fuzz = job.params.float("fuzz") as f32;
    match deft_tools_image::edit::trim_box(&img, fuzz) {
        Some((x, y, w, h)) => save(job, &img.crop_imm(x, y, w, h), format),
        None => Ok(save(job, &img, format)?.warn("the image is one colour; nothing to trim")),
    }
}
