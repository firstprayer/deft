use super::_common::{PASSWORD, open, plural, resolve, write};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "pdf.select",
    summary: "Keep and reorder pages",
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::one(&[Format::Pdf], "pdf"),
    params: &[
        param("pages", Kind::Pages, "Pages to keep, in output order").required(),
        PASSWORD,
    ],
    examples: &[
        "deft pdf select three-pages.pdf --pages 3,1-2 -o reordered.pdf",
        "deft pdf select report.pdf --pages 1",
    ],
    tags: &[
        "extract",
        "keep",
        "reorder",
        "rearrange",
        "pick",
        "pull out pages",
        "keep pages",
        "pages",
    ],
    result: &["pages"],
    suffix: Some("selected"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let mut doc = open(job.input(), job.params)?;
    let indexes = resolve(
        &job.params.pages("pages"),
        deft_tools_pdf::page_ids(&doc).len(),
    )?;
    deft_tools_pdf::select(&mut doc, &indexes)?;
    Ok(OpResult::one(write(&mut doc)?)
        .fact("pages", indexes.len())
        .note(plural(indexes.len(), "page")))
}
