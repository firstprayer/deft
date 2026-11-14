use super::_common::{PASSWORD, open, page_list, resolve};
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
    let mut text = String::new();
    let (mut found, mut scans) = (0, Vec::new());
    deft_tools_pdf::text::reader(&doc)?.each_page_text(&indexes, |i, page| {
        if !text.is_empty() {
            text.push_str("\x0c\n");
        }
        text.push_str(&page.text());
        text.push('\n');
        found += page.chars();
        if page.is_scan() {
            scans.push(i + 1);
        }
    });
    let chars = text.chars().count();
    let mut res = OpResult::one(Produced::bytes(text.clone().into_bytes(), Format::Txt))
        .fact("chars", chars)
        .note(format!("{chars} characters"));
    if found <= indexes.len() * 2 {
        res = res.warn(
            "no text found: the PDF has no text layer and may be a scan; deft pdf ocr reads it",
        );
    } else if !scans.is_empty() {
        res = res.warn(format!(
            "{} of {} pages are images with little or no text (page {}); deft pdf ocr reads them",
            scans.len(),
            indexes.len(),
            page_list(&scans)
        ));
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
