use super::_common::{IN, load};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "epub.to-text",
    summary: "Extract plain text in reading order",
    input: Input::each(IN, "epub"),
    output: Output::one(&[Format::Txt], "txt"),
    converts: &[Edge {
        from: IN,
        to: &[Format::Txt],
        quality: Quality::Approximate,
    }],
    examples: &["deft epub to-text book.epub -o book.txt"],
    tags: &["ebook to txt", "read book"],
    result: &["chapters"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let b = load(job.input())?;
    let doc = deft_tools_doc::Document {
        blocks: deft_tools_doc::markdown::parse(&b.markdown()?),
        ..Default::default()
    };
    Ok(
        OpResult::one(Produced::bytes(doc.text().into_bytes(), Format::Txt))
            .fact("chapters", b.chapters.len())
            .note(format!("{} chapters", b.chapters.len())),
    )
}
