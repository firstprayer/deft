use super::_common::{IN, NOTES, load};
use crate::generated::md::_common::save_images;
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "slides.to-md",
    summary: "Outline in Markdown: a heading per slide, bullets, images",
    input: Input::each(IN, "pptx"),
    output: Output::one(&[Format::Md], "md"),
    params: &[NOTES, crate::generated::doc::_common::IMAGES],
    converts: &[Edge {
        from: IN,
        to: &[Format::Md],
        quality: Quality::Approximate,
    }],
    examples: &["deft slides to-md deck.pptx -o deck.md"],
    tags: &["pptx to markdown", "outline", "summarize deck"],
    result: &["slides"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let (slides, doc) = load(job)?;
    let dir = job.params.opt_path("images");
    let mut md = deft_tools_doc::markdown::render(&doc);
    if let Some(d) = &dir {
        for image in &doc.images {
            md = md.replace(
                &format!("]({})", image.name),
                &format!("]({}/{})", d.display(), image.name),
            );
        }
    }
    let warning = save_images(&doc.images, dir)?;
    let mut res = OpResult::one(Produced::bytes(md.into_bytes(), Format::Md))
        .fact("slides", slides.len())
        .note(format!("{} slides", slides.len()));
    if let Some(w) = warning {
        res = res.warn(w);
    }
    Ok(res)
}
