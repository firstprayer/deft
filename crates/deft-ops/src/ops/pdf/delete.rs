use super::_common::{PASSWORD, open, plural, resolve_set, write};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "pdf.delete",
    summary: "Remove pages: --pages 2,5-7",
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::one(&[Format::Pdf], "pdf"),
    params: &[
        param("pages", Kind::Pages, "Pages to remove").required(),
        PASSWORD,
    ],
    examples: &[
        "deft pdf delete three-pages.pdf --pages 2 -o shorter.pdf",
        "deft pdf delete report.pdf --pages last",
    ],
    tags: &["remove pages", "drop"],
    result: &["pages"],
    suffix: Some("trimmed"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let mut doc = open(job.input(), job.params)?;
    let count = deft_tools_pdf::page_ids(&doc).len();
    let drop = resolve_set(&job.params.pages("pages"), count)?;
    let keep: Vec<usize> = (0..count).filter(|i| !drop.contains(i)).collect();
    deft_tools_pdf::select(&mut doc, &keep)?;
    Ok(OpResult::one(write(&mut doc)?)
        .fact("pages", keep.len())
        .note(format!("{} left", plural(keep.len(), "page"))))
}
