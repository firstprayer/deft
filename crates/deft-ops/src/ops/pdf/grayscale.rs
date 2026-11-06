use super::_common::PASSWORD;
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "pdf.grayscale",
    summary: "Remove colour (Ghostscript)",
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::one(&[Format::Pdf], "pdf"),
    params: &[PASSWORD],
    engines: &["gs"],
    examples: &["deft pdf grayscale photos.pdf -o gray.pdf"],
    tags: &["black and white", "print", "monochrome"],
    suffix: Some("gray"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let source = super::_gs::plain_copy(job)?;
    super::_gs::rewrite(
        job,
        OP.id,
        &source,
        &[
            "-sColorConversionStrategy=Gray",
            "-dProcessColorModel=/DeviceGray",
        ],
    )
}
