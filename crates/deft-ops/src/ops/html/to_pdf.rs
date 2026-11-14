use super::_common::{IN, content_with_pictures};
use crate::generated::md::_common::{FONT, FONT_SIZE, MARGIN, PAGE_SIZE, markdown_pdf};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "html.to-pdf",
    summary: "Typeset as PDF, content not layout",
    details: &["The page's CSS and scripts are not applied."],
    input: Input::each(IN, "html"),
    output: Output::one(&[Format::Pdf], "pdf"),
    params: &[PAGE_SIZE, MARGIN, FONT_SIZE, FONT],
    converts: &[Edge {
        from: IN,
        to: &[Format::Pdf],
        quality: Quality::Approximate,
    }],
    stability: Stability::Beta,
    examples: &["deft html to-pdf page.html -o page.pdf"],
    tags: &["html to pdf", "print page", "wkhtmltopdf"],
    result: &["pages"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let md = deft_tools_doc::html::to_markdown(&content_with_pictures(job)?)?;
    let base = job.input().path.as_ref().and_then(|p| p.parent());
    markdown_pdf(job, &md, base)
}
