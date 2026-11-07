use super::_common::{IN, load};
use crate::generated::md::_common::{
    FONT, FONT_SIZE, MARGIN, PAGE_SIZE, images_folder, markdown_pdf,
};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "doc.to-pdf",
    summary: "Typeset as PDF, content not layout",
    details: &["Headings, lists, tables and images survive; fonts, columns and colours do not."],
    input: Input::each(IN, "doc"),
    output: Output::one(&[Format::Pdf], "pdf"),
    params: &[PAGE_SIZE, MARGIN, FONT_SIZE, FONT],
    converts: &[Edge {
        from: IN,
        to: &[Format::Pdf],
        quality: Quality::Approximate,
    }],
    stability: Stability::Beta,
    examples: &["deft doc to-pdf letter.docx -o letter.pdf"],
    tags: &["word to pdf", "docx to pdf", "print", "office to pdf"],
    result: &["pages"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let doc = load(job.input())?;
    let dir = images_folder(job, &doc.images)?;
    let md = deft_tools_doc::markdown::render(&doc);
    let mut res = markdown_pdf(job, &md, Some(&dir))?;
    res.warnings.splice(0..0, doc.warnings);
    Ok(res)
}
