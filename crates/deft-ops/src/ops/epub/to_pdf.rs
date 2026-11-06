use super::_common::{IN, load};
use crate::generated::md::_common::{
    FONT, FONT_SIZE, MARGIN, PAGE_SIZE, images_folder, markdown_pdf,
};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "epub.to-pdf",
    summary: "Typeset the book as PDF",
    input: Input::each(IN, "epub"),
    output: Output::one(&[Format::Pdf], "pdf"),
    params: &[PAGE_SIZE, MARGIN, FONT_SIZE, FONT],
    converts: &[Edge {
        from: IN,
        to: &[Format::Pdf],
        quality: Quality::Faithful,
    }],
    examples: &["deft epub to-pdf book.epub -o book.pdf"],
    tags: &["ebook to pdf", "print book"],
    result: &["pages"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let b = load(job.input())?;
    let dir = images_folder(job, &b.images)?;
    markdown_pdf(job, &b.markdown()?, Some(&dir))
}
