use super::_common::{IN, MAIN, content};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "html.to-md",
    summary: "Convert to Markdown; scripts and styles are dropped",
    input: Input::each(IN, "html"),
    output: Output::one(&[Format::Md], "md"),
    mounts: &["md from-html"],
    params: &[MAIN],
    converts: &[Edge {
        from: IN,
        to: &[Format::Md],
        quality: Quality::Faithful,
    }],
    examples: &["deft html to-md page.html -o page.md"],
    tags: &[
        "html to markdown",
        "clean page",
        "readable",
        "web to markdown",
    ],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let md = deft_tools_doc::html::to_markdown(&content(job)?)?;
    Ok(OpResult::one(Produced::bytes(md.into_bytes(), Format::Md)))
}
