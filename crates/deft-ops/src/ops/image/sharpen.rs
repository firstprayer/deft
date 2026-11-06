use super::_common::{IN, INOUT, QUALITY, open, save};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "image.sharpen",
    summary: "Unsharp mask: --radius 2",
    input: Input::each(IN, "image"),
    output: Output::one(INOUT, "image"),
    params: &[
        param(
            "radius",
            Kind::Float {
                min: 0.1,
                max: 500.0,
            },
            "Larger means stronger edges",
        )
        .default("2"),
        QUALITY,
    ],
    examples: &["deft image sharpen photo-1.jpg -o crisp.jpg"],
    tags: &["unsharp", "crisp", "enhance", "detail"],
    result: &["width", "height"],
    suffix: Some("sharpened"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let (img, format) = open(job)?;
    let radius = job.params.float("radius") as f32;
    save(job, &deft_tools_image::edit::sharpen(&img, radius), format)
}
