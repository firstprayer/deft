use super::_common::{FUZZ, IN, QUALITY, open, save};
use deft_core::prelude::*;

const OUT: &[Format] = &[
    Format::Png,
    Format::Webp,
    Format::Avif,
    Format::Gif,
    Format::Tiff,
];

pub static OP: Op = Op {
    id: "image.transparent",
    summary: "Make a colour see-through: --color white --fuzz 10",
    details: &[
        "When most of the colour touches the picture's edge it is the background, and only the part joined to the edge goes: a white product on white keeps its own white. Otherwise it goes everywhere, like the paper of a scan in a dark border. --everywhere always removes it everywhere.",
        "JPEG cannot hold transparency, so JPEG input becomes PNG.",
    ],
    input: Input::each(IN, "image"),
    output: Output::one(OUT, "image"),
    params: &[
        param("color", Kind::Color, "Colour to remove").default("white"),
        FUZZ.default("10"),
        param(
            "everywhere",
            Kind::Flag,
            "Remove the colour inside the picture too",
        ),
        QUALITY,
    ],
    examples: &[
        "deft image transparent logo.png --color white -o cutout.png",
        "deft image transparent logo.png --everywhere -o see-through.png",
    ],
    tags: &[
        "transparent background",
        "chroma key",
        "green screen",
        "alpha",
        "color key",
    ],
    result: &["width", "height", "pixels"],
    suffix: Some("transparent"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let (img, format) = open(job)?;
    let fuzz = p.float("fuzz") as f32;
    let (out, n) = deft_tools_image::edit::make_transparent(
        &img,
        p.color("color"),
        fuzz,
        p.flag("everywhere"),
    );
    let format = if OUT.contains(&format) {
        format
    } else {
        Format::Png
    };
    Ok(save(job, &out, format)?.fact("pixels", n))
}
