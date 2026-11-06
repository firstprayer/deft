use super::_common::{PASSWORD, open, resolve};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "pdf.to-md",
    summary: "Text as Markdown paragraphs",
    details: &["Reflows lines into paragraphs. Tables and columns are not reconstructed."],
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::one(&[Format::Md], "md"),
    params: &[
        param("pages", Kind::Pages, "Pages to read").default("all"),
        PASSWORD,
    ],
    mounts: &["md from-pdf"],
    converts: &[Edge {
        from: &[Format::Pdf],
        to: &[Format::Md],
        quality: Quality::Approximate,
    }],
    stability: Stability::Beta,
    examples: &["deft pdf to-md report.pdf -o report.md"],
    tags: &["pdf to markdown", "llm", "reflow"],
    result: &["chars"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let doc = open(job.input(), job.params)?;
    let count = deft_tools_pdf::page_ids(&doc).len();
    let indexes = resolve(&job.params.pages("pages"), count)?;
    let mut out = String::new();
    for &i in &indexes {
        for p in deft_tools_pdf::reflow(&deft_tools_pdf::extract_text(&doc, &[i])?) {
            out.push_str(&p);
            out.push_str("\n\n");
        }
    }
    let chars = out.chars().count();
    let mut res = OpResult::one(Produced::bytes(out.into_bytes(), Format::Md))
        .fact("chars", chars)
        .note(format!("{chars} characters"));
    if chars <= indexes.len() * 2 {
        res = res.warn("no text found; the PDF may be a scan");
    }
    Ok(res)
}
