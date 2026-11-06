use super::_common::{IN, QUALITY, open};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "image.convert",
    summary: "Change format: --to png|jpg|webp|avif|gif|tiff|bmp|ico",
    details: &["WebP output is lossless. Transparent areas become --background in JPEG."],
    input: Input::each(IN, "image"),
    output: Output::one(super::_common::INOUT, "image"),
    params: &[
        param(
            "to",
            Kind::Choice(&["png", "jpg", "webp", "avif", "gif", "tiff", "bmp", "ico"]),
            "Target format (or give -o name.ext)",
        ),
        QUALITY,
        param(
            "background",
            Kind::Color,
            "Fill for transparent areas in JPEG",
        )
        .default("white"),
    ],
    converts: &[Edge {
        from: deft_tools_image::READABLE,
        to: super::_common::INOUT,
        quality: Quality::Faithful,
    }],
    examples: &[
        "deft image convert photo-1.jpg --to png -o photo.png",
        "deft image convert *.jpg --to webp -o web/",
    ],
    tags: &[
        "format", "png", "jpg", "jpeg", "webp", "avif", "ico", "tiff", "bmp", "gif", "svg",
    ],
    result: &["width", "height"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let to = p.opt_text("to").ok_or_else(|| {
        err(
            ErrorCode::BadOption,
            "--to is required (or give -o name.ext)",
        )
    })?;
    let format = Format::from_name(&to).expect("choice");
    let (img, _) = open(job)?;
    let [r, g, b, _] = p.color("background");
    let opt = deft_tools_image::Encode {
        quality: p.int("quality") as u8,
        background: [r, g, b],
    };
    let bytes = deft_tools_image::encode(&img, format, opt)?;
    let (w, h) = (img.width(), img.height());
    Ok(OpResult::one(Produced::bytes(bytes, format))
        .fact("width", w)
        .fact("height", h)
        .note(format!("{w}x{h}")))
}
