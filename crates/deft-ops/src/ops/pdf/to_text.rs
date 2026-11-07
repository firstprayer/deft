use super::_common::{PASSWORD, open, resolve};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "pdf.to-text",
    summary: "Extract text",
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::one(&[Format::Txt], "txt"),
    params: &[
        param("pages", Kind::Pages, "Pages to read").default("all"),
        PASSWORD,
    ],
    converts: &[Edge {
        from: &[Format::Pdf],
        to: &[Format::Txt],
        quality: Quality::Approximate,
    }],
    examples: &[
        "deft pdf to-text report.pdf -o report.txt",
        "deft pdf to-text three-pages.pdf --pages 2 -o -",
    ],
    tags: &[
        "extract text",
        "read",
        "ocr",
        "txt",
        "get text",
        "text out",
        "read",
    ],
    result: &["chars"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let doc = open(job.input(), job.params)?;
    let indexes = resolve(
        &job.params.pages("pages"),
        deft_tools_pdf::page_ids(&doc).len(),
    )?;
    let text = deft_tools_pdf::extract_text(&doc, &indexes)?;
    let chars = text.chars().count();
    let mut res = OpResult::one(Produced::bytes(text.clone().into_bytes(), Format::Txt))
        .fact("chars", chars)
        .note(format!("{chars} characters"));
    if chars <= indexes.len() * 2 {
        res = res.warn("no text found; the PDF may be a scan");
    }
    let lost = deft_tools_pdf::text::undecoded(&text);
    if lost > 0.05 {
        res = res.warn(format!(
            "{:.0}% of the text could not be decoded; pdf ocr reads the pages instead",
            lost * 100.0
        ));
    }
    Ok(res)
}
