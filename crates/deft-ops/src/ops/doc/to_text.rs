use super::_common::{IN, load};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "doc.to-text",
    summary: "Extract plain text",
    input: Input::each(IN, "doc"),
    output: Output::one(&[Format::Txt], "txt"),
    converts: &[Edge {
        from: IN,
        to: &[Format::Txt],
        quality: Quality::Approximate,
    }],
    examples: &["deft doc to-text letter.docx -o letter.txt"],
    tags: &["read word", "docx to txt", "extract text"],
    result: &["words"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let doc = load(job.input())?;
    let words = doc.words();
    Ok(
        OpResult::one(Produced::bytes(doc.text().into_bytes(), Format::Txt))
            .fact("words", words)
            .note(format!("{words} words")),
    )
}
