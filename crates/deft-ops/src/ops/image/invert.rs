use super::_common::{IN, INOUT, QUALITY, open, save};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "image.invert",
    summary: "Negative: flip every colour",
    input: Input::each(IN, "image"),
    output: Output::one(INOUT, "image"),
    params: &[QUALITY],
    examples: &["deft image invert logo.png -o dark.png"],
    tags: &["negative", "negate", "dark mode"],
    result: &["width", "height"],
    suffix: Some("inverted"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let (mut img, format) = open(job)?;
    img.invert();
    save(job, &img, format)
}
