use super::_common::{FONT, FONT_SIZE, MARGIN, PAGE_SIZE, markdown_pdf};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "md.to-pdf",
    summary: "Typeset Markdown as PDF",
    details: &[
        "Relative image paths resolve against the Markdown file. Non-Latin text uses system fonts.",
    ],
    input: Input::each(&[Format::Md, Format::Txt], "md"),
    output: Output::one(&[Format::Pdf], "pdf"),
    params: &[PAGE_SIZE, MARGIN, FONT_SIZE, FONT],
    mounts: &["pdf from-md"],
    converts: &[Edge {
        from: &[Format::Md],
        to: &[Format::Pdf],
        quality: Quality::Faithful,
    }],
    examples: &[
        "deft md to-pdf notes.md -o notes.pdf",
        "deft pdf from-md notes.md --page-size letter --font-size 12 -o letter.pdf",
    ],
    tags: &["markdown to pdf", "typeset", "print", "report"],
    result: &["pages"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let input = job.input();
    let source = String::from_utf8_lossy(input.bytes()?).to_string();
    let (_, md) = deft_tools_doc::markdown::split_front_matter(&source);
    let base = input
        .path
        .as_ref()
        .and_then(|x| x.parent())
        .map(|x| x.to_path_buf());
    markdown_pdf(job, md, base.as_deref())
}
