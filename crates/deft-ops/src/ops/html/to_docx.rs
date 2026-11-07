use super::_common::{IN, MAIN, content, text};
use crate::generated::md::_common::{load_images, svg_fallbacks};
use deft_core::prelude::*;
use deft_tools_doc::Document;

pub static OP: Op = Op {
    id: "html.to-docx",
    summary: "Convert the content to a Word document",
    input: Input::each(IN, "html"),
    output: Output::one(&[Format::Docx], "docx"),
    mounts: &["doc from-html"],
    params: &[MAIN],
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
    let md = deft_tools_doc::html::to_markdown(&content(job)?)?;
    let mut blocks = deft_tools_doc::markdown::parse(&md);
    let base = job.input().path.as_ref().and_then(|p| p.parent());
    let (images, mut warnings) = load_images(&mut blocks, base);
    let (fallbacks, problems) = svg_fallbacks(&images);
    warnings.extend(problems);
    let doc = Document {
        fallbacks,
        title: deft_tools_doc::html::title(&text(job.input())?),
        blocks,
        images,
        ..Default::default()
    };
    let mut res = OpResult::one(Produced::bytes(
        deft_tools_doc::docx::write(&doc)?,
        Format::Docx,
    ));
    res.warnings = warnings;
    Ok(res)
}
