use deft_core::prelude::*;
use deft_tools_doc::pptx::read;

pub static OP: Op = Op {
    id: "slides.info",
    summary: "Slide count, titles, images, notes",
    input: Input::each(super::_common::IN, "pptx"),
    output: Output::report(),
    examples: &["deft slides info deck.pptx"],
    tags: &["how many slides", "outline", "titles"],
    result: &["slides", "titles", "images", "notes"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let (slides, images) = read(job.input().bytes()?)?;
    let titles: Vec<String> = slides
        .iter()
        .enumerate()
        .map(|(i, s)| {
            s.title
                .clone()
                .unwrap_or_else(|| format!("Slide {}", i + 1))
        })
        .collect();
    let notes = slides.iter().filter(|s| s.notes.is_some()).count();
    Ok(OpResult::report(
        facts![
            ("slides", slides.len()),
            ("titles", titles),
            ("images", images.len()),
            ("notes", notes)
        ],
        format!("{} slides", slides.len()),
    ))
}
