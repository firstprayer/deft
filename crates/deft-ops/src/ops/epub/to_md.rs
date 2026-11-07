use super::_common::{IN, load};
use crate::generated::md::_common::{folder_link, relink_images, save_images};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "epub.to-md",
    summary: "Convert the whole book to Markdown",
    input: Input::each(IN, "epub"),
    output: Output::one(&[Format::Md], "md"),
    params: &[crate::generated::doc::_common::IMAGES],
    converts: &[Edge {
        from: IN,
        to: &[Format::Md],
        quality: Quality::Faithful,
    }],
    examples: &["deft epub to-md book.epub -o book.md"],
    tags: &["ebook to markdown", "read book"],
    result: &["chapters"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let b = load(job.input())?;
    let mut md = b.markdown()?;
    let dir = job.params.opt_path("images");
    if let Some(d) = &dir {
        md = relink_images(&md, &b.images, &folder_link(job, d));
    }
    let warning = save_images(&b.images, dir)?;
    let mut res = OpResult::one(Produced::bytes(md.into_bytes(), Format::Md))
        .fact("chapters", b.chapters.len())
        .note(format!("{} chapters", b.chapters.len()));
    if let Some(w) = warning {
        res = res.warn(w);
    }
    Ok(res)
}
