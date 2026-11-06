use super::_common::load_images;
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "md.to-pptx",
    summary: "Make a deck: each heading or --- starts a slide",
    details: &["Lists become bullets; images sit beside the text. A plain theme is used."],
    input: Input::each(&[Format::Md, Format::Txt], "md"),
    output: Output::one(&[Format::Pptx], "pptx"),
    mounts: &["slides from-md"],
    converts: &[Edge {
        from: &[Format::Md],
        to: &[Format::Pptx],
        quality: Quality::Faithful,
    }],
    stability: Stability::Beta,
    examples: &["deft md to-pptx notes.md -o notes.pptx"],
    tags: &["markdown to powerpoint", "slides", "deck", "presentation"],
    result: &["slides"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let input = job.input();
    let md = String::from_utf8_lossy(input.bytes()?).to_string();
    let base = input.path.as_ref().and_then(|x| x.parent());
    let mut blocks = deft_tools_doc::markdown::parse(&md);
    let images = load_images(&mut blocks, base);
    let slides = deft_tools_doc::pptx::slides_from(&blocks);
    if slides.is_empty() {
        return Err(err(ErrorCode::InputCorrupt, "nothing to put on a slide"));
    }
    let n = slides.len();
    let bytes = deft_tools_doc::pptx::write(&slides, &images)?;
    Ok(OpResult::one(Produced::bytes(bytes, Format::Pptx))
        .fact("slides", n)
        .note(format!("{n} slides")))
}
