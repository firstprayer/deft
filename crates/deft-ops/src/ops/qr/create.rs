use deft_core::prelude::*;
use deft_tools_image::codes;

pub static OP: Op = Op {
    id: "qr.create",
    summary: "Make a QR code or barcode: --text https://example.com",
    details: &[
        "--type code128, code39, ean13, ean8 or itf makes a linear barcode.",
        "--module-size is pixels per module; SVG output ignores it.",
    ],
    output: Output::one(
        &[Format::Png, Format::Svg, Format::Jpeg, Format::Webp],
        "image",
    ),
    params: &[
        param("text", Kind::Text, "What to encode").required(),
        param(
            "type",
            Kind::Choice(&["qr", "code128", "code39", "ean13", "ean8", "itf"]),
            "Kind of code",
        )
        .default("qr"),
        param(
            "correction",
            Kind::Choice(&["l", "m", "q", "h"]),
            "QR error correction",
        )
        .default("m"),
        param(
            "module-size",
            Kind::Int { min: 1, max: 100 },
            "Pixels per module",
        )
        .default("8"),
        param("color", Kind::Color, "Dark modules").default("black"),
        param("background", Kind::Color, "Light modules").default("white"),
    ],
    examples: &[
        "deft qr create --text https://example.com -o link.png",
        "deft qr create --text ABC-123 --type code128 -o label.svg",
    ],
    tags: &["qr code", "barcode", "generate", "ean", "upc", "wifi code"],
    result: &["modules"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let text = p.text("text");
    let kind = p.text("type");
    let m = if kind == "qr" {
        codes::qr(&text, &p.text("correction"))?
    } else {
        codes::barcode(&text, &kind)?
    };
    let (dark, light) = (p.color("color"), p.color("background"));
    let format = job.target(Format::Png);
    let res = if format == Format::Svg {
        OpResult::one(Produced::bytes(
            codes::svg(&m, dark, light).into_bytes(),
            Format::Svg,
        ))
    } else {
        let img = codes::draw(&m, p.int("module-size") as u32, dark, light);
        let bytes = deft_tools_image::encode(&img, format, Default::default())?;
        OpResult::one(Produced::bytes(bytes, format))
    };
    Ok(res.fact("modules", m.width))
}
