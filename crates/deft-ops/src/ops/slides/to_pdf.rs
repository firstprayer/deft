use super::_common::{IN, NOTES, load};
use crate::generated::md::_common::{
    FONT, FONT_SIZE, MARGIN, PAGE_SIZE, images_folder, markdown_pdf,
};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "slides.to-pdf",
    summary: "Handout PDF of slide content",
    details: &["This is the content, not a picture of the slide: layouts and themes are lost."],
    input: Input::each(IN, "pptx"),
    output: Output::one(&[Format::Pdf], "pdf"),
    params: &[NOTES, PAGE_SIZE, MARGIN, FONT_SIZE, FONT],
    converts: &[Edge {
        from: IN,
        to: &[Format::Pdf],
        quality: Quality::Approximate,
    }],
    stability: Stability::Beta,
    examples: &["deft slides to-pdf deck.pptx -o handout.pdf"],
    tags: &["pptx to pdf", "powerpoint to pdf", "handout"],
    result: &["pages"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let (_, doc) = load(job)?;
    let dir = images_folder(job, &doc.images)?;
    let md = deft_tools_doc::markdown::render(&doc);
    markdown_pdf(job, &md, Some(&dir))
}
