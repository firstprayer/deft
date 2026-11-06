use super::_common::{IN, INOUT, QUALITY, open, save};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "image.flip",
    summary: "Mirror: --direction horizontal|vertical",
    input: Input::each(IN, "image"),
    output: Output::one(INOUT, "image"),
    params: &[
        param(
            "direction",
            Kind::Choice(&["horizontal", "vertical"]),
            "Mirror axis",
        )
        .default("horizontal"),
        QUALITY,
    ],
    examples: &["deft image flip photo-1.jpg -o mirrored.jpg"],
    tags: &["mirror", "reverse"],
    result: &["width", "height"],
    suffix: Some("flipped"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let (img, format) = open(job)?;
    let out = if job.params.text("direction") == "vertical" {
        img.flipv()
    } else {
        img.fliph()
    };
    save(job, &out, format)
}
