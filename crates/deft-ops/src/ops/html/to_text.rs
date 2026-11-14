use super::_common::{IN, content};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "html.to-text",
    summary: "Extract the readable text",
    input: Input::each(IN, "html"),
    output: Output::one(&[Format::Txt], "txt"),
    params: &[],
    converts: &[Edge {
        from: IN,
        to: &[Format::Txt],
        quality: Quality::Approximate,
    }],
    examples: &["deft html to-text page.html -o page.txt"],
    tags: &["strip tags", "html to txt", "scrape text"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let out = deft_tools_doc::html::to_text(&content(job)?)?;
    Ok(OpResult::one(Produced::bytes(
        out.into_bytes(),
        Format::Txt,
    )))
}
