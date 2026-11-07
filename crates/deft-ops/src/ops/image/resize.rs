use super::_common::{IN, INOUT, QUALITY, open, save};
use deft_core::prelude::*;
use deft_tools_image::Fit;

pub static OP: Op = Op {
    id: "image.resize",
    summary: "Scale: --size 800x, 1920x1080, 50%",
    details: &["Images never grow unless --upscale is set. SVG is drawn at the size asked for."],
    input: Input::each(IN, "image"),
    output: Output::one(INOUT, "image"),
    params: &[
        param("size", Kind::Size, "Target size").required(),
        param(
            "fit",
            Kind::Choice(&["contain", "cover", "fill"]),
            "For WxH: fit inside, fill and crop, or stretch",
        )
        .default("contain"),
        param("upscale", Kind::Flag, "Allow enlarging"),
        QUALITY,
    ],
    examples: &[
        "deft image resize photo-1.jpg --size 800x -o small.jpg",
        "deft image resize *.jpg --size 300x300 --fit cover -o thumbs/",
    ],
    tags: &[
        "scale",
        "thumbnail",
        "shrink",
        "smaller",
        "dimensions",
        "pixels",
        "wide",
        "tall",
    ],
    result: &["width", "height"],
    suffix: Some("resized"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let (spec, fit) = (p.size("size"), Fit::parse(&p.text("fit")));
    let input = job.input();
    if input.require_format()? == Format::Svg {
        let max = job.ctx.limits.max_pixels;
        let out = deft_tools_image::render_svg_sized(input.bytes()?, &spec, fit, max)?;
        return save(job, &out, Format::Svg);
    }
    let (img, format) = open(job)?;
    let out = deft_tools_image::resize(&img, &spec, fit, p.flag("upscale"))?;
    save(job, &out, format)
}
