use super::_common::{IN, INOUT, QUALITY, open, save};
use deft_core::prelude::*;

const PERCENT: Kind = Kind::Float {
    min: -100.0,
    max: 100.0,
};

pub static OP: Op = Op {
    id: "image.adjust",
    summary: "Change --brightness, --contrast, --saturation, --gamma",
    details: &["The first three go from -100 to 100; 0 changes nothing. Gamma 1 changes nothing."],
    input: Input::each(IN, "image"),
    output: Output::one(INOUT, "image"),
    params: &[
        param("brightness", PERCENT, "Percent, -100 to 100").default("0"),
        param("contrast", PERCENT, "Percent, -100 to 100").default("0"),
        param("saturation", PERCENT, "Percent, -100 to 100").default("0"),
        param(
            "gamma",
            Kind::Float {
                min: 0.1,
                max: 10.0,
            },
            "Above 1 lifts the midtones",
        )
        .default("1"),
        QUALITY,
    ],
    examples: &["deft image adjust photo-1.jpg --brightness 15 --contrast 10 -o lifted.jpg"],
    tags: &[
        "lighten",
        "darken",
        "vivid",
        "levels",
        "color correction",
        "exposure",
    ],
    result: &["width", "height"],
    suffix: Some("adjusted"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let (img, format) = open(job)?;
    let out = deft_tools_image::edit::adjust(
        &img,
        p.float("brightness") as f32,
        p.float("contrast") as f32,
        p.float("saturation") as f32,
        p.float("gamma") as f32,
    );
    save(job, &out, format)
}
