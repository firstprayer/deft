use super::_common::{PASSWORD, open};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "pdf.fields",
    summary: "List form fields",
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::report(),
    params: &[PASSWORD],
    examples: &["deft pdf fields form.pdf --json"],
    tags: &["form", "acroform", "inputs", "what can I fill"],
    result: &["fields"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let doc = open(job.input(), job.params)?;
    let list: Vec<_> = deft_tools_pdf::forms::fields(&doc)
        .iter()
        .map(|f| f.to_json())
        .collect();
    let note = format!("{} fields", list.len());
    Ok(OpResult::report(facts![("fields", list)], note))
}
