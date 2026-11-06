use super::_common::{IN, text};
use crate::generated::md::_common::load_images;
use deft_core::prelude::*;
use deft_tools_doc::Document;

pub static OP: Op = Op {
    id: "html.to-docx",
    summary: "Convert the content to a Word document",
    input: Input::each(IN, "html"),
    output: Output::one(&[Format::Docx], "docx"),
    mounts: &["doc from-html"],
    converts: &[Edge {
        from: IN,
        to: &[Format::Docx],
        quality: Quality::Faithful,
    }],
    examples: &["deft html to-docx page.html -o page.docx"],
    tags: &["html to word", "web page to word"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let html = text(job.input())?;
    let md = deft_tools_doc::html::to_markdown(&html)?;
    let mut blocks = deft_tools_doc::markdown::parse(&md);
    let base = job.input().path.as_ref().and_then(|p| p.parent());
    let images = load_images(&mut blocks, base);
    let doc = Document {
        title: deft_tools_doc::html::title(&html),
        blocks,
        images,
        ..Default::default()
    };
    Ok(OpResult::one(Produced::bytes(
        deft_tools_doc::docx::write(&doc)?,
        Format::Docx,
    )))
}
