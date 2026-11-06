use super::_common::{IN, load};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "doc.to-html",
    summary: "Convert to a web page",
    details: &["Images are not embedded; use doc to-md --images to keep them."],
    input: Input::each(IN, "doc"),
    output: Output::one(&[Format::Html], "html"),
    params: &[param("fragment", Kind::Flag, "Body only, no <html> page")],
    converts: &[Edge {
        from: IN,
        to: &[Format::Html],
        quality: Quality::Faithful,
    }],
    examples: &["deft doc to-html letter.docx -o letter.html"],
    tags: &["docx to html", "web page", "publish"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let doc = load(job.input())?;
    let md = deft_tools_doc::markdown::render(&doc);
    let html = deft_tools_text::to_html(&md, !job.params.flag("fragment"), doc.title.as_deref());
    Ok(OpResult::one(Produced::bytes(
        html.into_bytes(),
        Format::Html,
    )))
}
