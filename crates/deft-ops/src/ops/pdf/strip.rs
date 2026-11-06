use super::_common::{PASSWORD, open, write};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "pdf.strip",
    summary: "Remove metadata",
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::one(&[Format::Pdf], "pdf"),
    params: &[PASSWORD],
    examples: &["deft pdf strip report.pdf -o clean.pdf"],
    tags: &["metadata", "privacy", "anonymize", "author"],
    suffix: Some("stripped"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let mut doc = open(job.input(), job.params)?;
    deft_tools_pdf::meta::strip(&mut doc);
    Ok(OpResult::one(write(&mut doc)?))
}
