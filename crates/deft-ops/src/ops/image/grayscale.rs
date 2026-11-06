use super::_common::{IN, INOUT, QUALITY, open, save};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "image.grayscale",
    summary: "Remove colour",
    input: Input::each(IN, "image"),
    output: Output::one(INOUT, "image"),
    params: &[QUALITY],
    examples: &["deft image grayscale photo-1.jpg -o mono.jpg"],
    tags: &["black and white", "greyscale", "monochrome", "desaturate"],
    result: &["width", "height"],
    suffix: Some("gray"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let (img, format) = open(job)?;
    save(job, &img.grayscale(), format)
}
