use super::_common::{PASSWORD, open, write};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "pdf.flatten",
    summary: "Bake forms into pages",
    details: &["Nothing can be edited afterwards. Links stay."],
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::one(&[Format::Pdf], "pdf"),
    params: &[PASSWORD],
    examples: &["deft pdf flatten form.pdf -o flat.pdf"],
    tags: &[
        "forms",
        "annotations",
        "comments",
        "uneditable",
        "lock form",
    ],
    result: &["flattened"],
    suffix: Some("flat"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let mut doc = open(job.input(), job.params)?;
    let n = deft_tools_pdf::forms::flatten(&mut doc)?;
    Ok(OpResult::one(write(&mut doc)?)
        .fact("flattened", n)
        .note(format!("{n} annotations")))
}
