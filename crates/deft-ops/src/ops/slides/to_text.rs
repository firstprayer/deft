use super::_common::{IN, NOTES, load};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "slides.to-text",
    summary: "Extract the text of every slide, form feed between slides",
    input: Input::each(IN, "pptx"),
    output: Output::one(&[Format::Txt], "txt"),
    params: &[NOTES],
    converts: &[Edge {
        from: IN,
        to: &[Format::Txt],
        quality: Quality::Approximate,
    }],
    examples: &["deft slides to-text deck.pptx --notes -o deck.txt"],
    tags: &["read powerpoint", "pptx to txt", "speaker notes"],
    result: &["slides"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let (slides, doc) = load(job)?;
    Ok(
        OpResult::one(Produced::bytes(doc.text().into_bytes(), Format::Txt))
            .fact("slides", slides.len())
            .note(format!("{} slides", slides.len())),
    )
}
