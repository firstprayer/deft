use super::_common::{IN, load};
use deft_core::prelude::*;
use deft_tools_doc::Block;

pub static OP: Op = Op {
    id: "doc.info",
    summary: "Title, author, words, headings, tables, images",
    input: Input::each(IN, "doc"),
    output: Output::report(),
    examples: &["deft doc info letter.docx"],
    tags: &["word count", "metadata", "properties", "outline"],
    result: &[
        "title",
        "author",
        "words",
        "paragraphs",
        "headings",
        "tables",
        "images",
    ],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let doc = load(job.input())?;
    let count = |f: fn(&Block) -> bool| doc.blocks.iter().filter(|b| f(b)).count();
    let words = doc.words();
    Ok(OpResult::report(
        facts![
            ("title", doc.title.clone()),
            ("author", doc.author.clone()),
            ("words", words),
            ("paragraphs", count(|b| matches!(b, Block::Paragraph(_)))),
            ("headings", count(|b| matches!(b, Block::Heading(..)))),
            ("tables", count(|b| matches!(b, Block::Table(_)))),
            ("images", doc.images.len()),
        ],
        format!("{words} words"),
    ))
}
