use super::_common::{IN, QUALITY, encode, open, profile};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "image.convert",
    summary: "Change format: --to png|jpg|webp|avif|gif|tiff|bmp|ico",
    details: &[
        "Photos become lossy WebP (needs ffmpeg), PNG and GIF lossless, unless --quality is given.",
        "Transparent areas become --background in JPEG.",
    ],
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
        from: IN,
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
    let icc = profile(job);
    let opt = deft_tools_image::Encode {
        quality: p.opt_int("quality").unwrap_or(85) as u8,
        background: [r, g, b],
        icc: icc.as_deref(),
    };
    let (bytes, warnings) = encode(job, &img, format, opt)?;
    let (w, h) = (img.width(), img.height());
    let mut res = OpResult::one(Produced::bytes(bytes, format))
        .fact("width", w)
        .fact("height", h)
        .note(format!("{w}x{h}"));
    for warning in warnings {
        res = res.warn(warning);
    }
    Ok(res)
}
