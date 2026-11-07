use super::_common::{IN, INOUT, QUALITY, open, save};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "image.blur",
    summary: "Gaussian blur: --radius 5, or a --box region",
    input: Input::each(IN, "image"),
    output: Output::one(INOUT, "image"),
    params: &[
        param(
            "radius",
            Kind::Float {
                min: 0.1,
                max: 500.0,
            },
            "Strength in pixels (default 5; with --box, enough to hide text)",
        ),
        param("box", Kind::Text, "Only this region: x,y,width,height"),
        QUALITY,
    ],
    examples: &[
        "deft image blur photo-1.jpg --radius 8 -o soft.jpg",
        "deft image blur photo-1.jpg --box 100,100,200,80 -o hidden.jpg",
    ],
    tags: &["gaussian", "soften", "censor", "hide plate", "hide face"],
    result: &["width", "height"],
    suffix: Some("blurred"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let (img, format) = open(job)?;
    let region = match job.params.opt_text("box") {
        Some(b) => {
            let v: Vec<u32> = b
                .split(',')
                .map(|s| s.trim().parse())
                .collect::<std::result::Result<_, _>>()
                .map_err(|_| bad_value("box", "use x,y,width,height in pixels"))?;
            let [x, y, w, h] = v[..] else {
                return Err(bad_value("box", "use x,y,width,height in pixels"));
            };
            if w == 0 || h == 0 || x + w > img.width() || y + h > img.height() {
                return Err(bad_value(
                    "box",
                    format!(
                        "region is outside the {}x{} image",
                        img.width(),
                        img.height()
                    ),
                ));
            }
            Some((x, y, w, h))
        }
        None => None,
    };
    // A box is there to hide something, and 5 leaves text readable.
    let radius = job.params.opt_float("radius").map_or_else(
        || region.map_or(5.0, |(_, _, _, h)| (h as f32 / 4.0).max(12.0)),
        |r| r as f32,
    );
    save(
        job,
        &deft_tools_image::edit::blur(&img, radius, region),
        format,
    )
}
