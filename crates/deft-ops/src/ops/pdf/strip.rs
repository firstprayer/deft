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
    let clean = deft_tools_pdf::meta::strip(&mut doc);
    let mut res = OpResult::one(write(&mut doc)?);
    if !clean {
        res = res.warn("this XFA form keeps its own details; they are the form itself");
    }
    Ok(res)
}
