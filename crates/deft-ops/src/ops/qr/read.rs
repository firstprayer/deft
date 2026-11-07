use deft_core::prelude::*;

const IN: &[Format] = &[
    Format::Png,
    Format::Jpeg,
    Format::Webp,
    Format::Gif,
    Format::Tiff,
    Format::Bmp,
    Format::Svg,
    Format::Heic,
    Format::Avif,
];

pub static OP: Op = Op {
    id: "qr.read",
    summary: "Decode every QR code in an image",
    details: &["Reads QR codes only; linear barcodes are not recognised."],
    input: Input::each(IN, "image"),
    output: Output::report(),
    examples: &["deft qr read qr.png"],
    tags: &["scan", "decode qr", "qr to text"],
    result: &["codes"],
    errors: &[ErrorCode::InputUnsupported],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let img = crate::generated::image::_common::decode(job, job.input())?;
    let codes = deft_tools_image::codes::read_qr(&img);
    let n = codes.len();
    let mut res = OpResult::report(facts![("codes", codes)], format!("{n} found"));
    if n == 0 {
        res = res.warn("no QR code found");
    }
    Ok(res)
}
