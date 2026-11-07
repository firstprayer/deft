use super::_common::{IN, open};
use deft_core::prelude::*;
use deft_tools_image::Fit;

const PNGS: &[(&str, u32)] = &[
    ("favicon-16x16.png", 16),
    ("favicon-32x32.png", 32),
    ("apple-touch-icon.png", 180),
    ("android-chrome-192x192.png", 192),
    ("android-chrome-512x512.png", 512),
];

pub static OP: Op = Op {
    id: "image.favicon",
    summary: "Website icon set: favicon.ico and PNG sizes",
    details: &["Non-square images are centred on a transparent square."],
    input: Input::each(IN, "image"),
    output: Output::many(&[Format::Ico, Format::Png], "ico+png"),
    examples: &["deft image favicon logo.png -o icons/"],
    tags: &["icon", "ico", "apple touch icon", "site icon"],
    result: &["files"],
    suffix: Some("favicon"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let input = job.input();
    let img = if input.require_format()? == Format::Svg {
        // Drawn at the largest icon's size, so the big icons are sharp.
        let spec = SizeSpec::parse("512x512").map_err(|m| bad_value("size", m))?;
        let max = job.ctx.limits.max_pixels;
        deft_tools_image::render_svg_sized(input.bytes()?, &spec, Fit::Contain, max)?
    } else {
        open(job)?.0
    };
    let side = img.width().max(img.height());
    let square = deft_tools_image::edit::pad(
        &img,
        side,
        side,
        [0, 0, 0, 0],
        deft_core::value::Position { x: 0.5, y: 0.5 },
    );
    let ico = deft_tools_image::edit::encode_ico(&square, &[16, 32, 48])?;
    let mut outputs = vec![Produced::bytes(ico, Format::Ico).named("favicon.ico")];
    for (name, size) in PNGS {
        let small = deft_tools_image::resize_exact(&square, *size, *size)?;
        let bytes = deft_tools_image::encode(&small, Format::Png, Default::default())?;
        outputs.push(Produced::bytes(bytes, Format::Png).named(*name));
    }
    let n = outputs.len();
    Ok(OpResult::many(outputs)
        .fact("files", n)
        .note(format!("{n} files")))
}
