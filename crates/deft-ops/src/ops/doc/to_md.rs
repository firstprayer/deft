use super::_common::{IMAGES, IN, load};
use crate::generated::md::_common::save_images;
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "doc.to-md",
    summary: "Convert to Markdown, keeping headings, lists and tables",
    input: Input::each(IN, "doc"),
    output: Output::one(&[Format::Md], "md"),
    params: &[IMAGES],
    mounts: &["md from-docx"],
    converts: &[Edge {
        from: IN,
        to: &[Format::Md],
        quality: Quality::Faithful,
    }],
    examples: &[
        "deft doc to-md letter.docx -o letter.md",
        "deft doc to-md letter.docx --images media -o with-pictures.md",
    ],
    tags: &["docx to markdown", "word to markdown", "pandoc"],
    result: &["words", "images"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let doc = load(job.input())?;
    let dir = job.params.opt_path("images");
    let mut md = deft_tools_doc::markdown::render(&doc);
    if let Some(d) = &dir {
        // Point the links at the folder the pictures went to.
        for image in &doc.images {
            md = md.replace(
                &format!("]({})", image.name),
                &format!("]({}/{})", d.display(), image.name),
            );
        }
    }
    let warning = save_images(&doc.images, dir)?;
    let words = doc.words();
    let mut res = OpResult::one(Produced::bytes(md.into_bytes(), Format::Md))
        .fact("words", words)
        .fact("images", doc.images.len())
        .note(format!("{words} words"));
    if let Some(w) = warning {
        res = res.warn(w);
    }
    res.warnings.extend(doc.warnings);
    Ok(res)
}
