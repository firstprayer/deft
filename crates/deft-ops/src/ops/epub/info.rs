use super::_common::{IN, load};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "epub.info",
    summary: "Title, author, language, chapters, images, words",
    input: Input::each(IN, "epub"),
    output: Output::report(),
    examples: &["deft epub info book.epub"],
    tags: &["metadata", "author", "book details"],
    result: &["title", "author", "language", "chapters", "images", "words"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let b = load(job.input())?;
    let words = deft_core::text::count_words(&b.markdown()?);
    let note = format!("{} chapters", b.chapters.len());
    Ok(OpResult::report(
        facts![
            ("title", b.title),
            ("author", b.author),
            ("language", b.language),
            ("chapters", b.chapters.len()),
            ("images", b.images.len()),
            ("words", words)
        ],
        note,
    ))
}
