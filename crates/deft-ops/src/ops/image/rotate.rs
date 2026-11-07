use super::_common::{IN, INOUT, QUALITY, open, save};
use deft_core::prelude::*;
use deft_tools_image::image::DynamicImage;

pub static OP: Op = Op {
    id: "image.rotate",
    summary: "Turn clockwise by any angle (EXIF is applied first)",
    details: &["--angle 0 only makes the pixels upright, for tools that ignore EXIF."],
    input: Input::each(IN, "image"),
    output: Output::one(INOUT, "image"),
    params: &[
        param(
            "angle",
            Kind::Float {
                min: -360.0,
                max: 360.0,
            },
            "Degrees clockwise; angles other than 90, 180, 270 grow the canvas",
        )
        .default("90"),
        QUALITY,
    ],
    examples: &[
        "deft image rotate photo-1.jpg --angle 180 -o upside.jpg",
        "deft image rotate photo-1.jpg --angle 12.5 -o tilted.png",
    ],
    tags: &["turn", "orientation", "upright"],
    result: &["width", "height"],
    suffix: Some("rotated"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let (img, format) = open(job)?;
    let angle = job.params.float("angle");
    if angle % 90.0 != 0.0 {
        let turned = deft_tools_image::rotate_any(&img.to_rgba8(), -angle as f32);
        return save(job, &DynamicImage::ImageRgba8(turned), format);
    }
    let out = match right_angle(angle)? {
        0 => img,
        180 => img.rotate180(),
        270 => img.rotate270(),
        _ => img.rotate90(),
    };
    save(job, &out, format)
}
