use super::_common::{IN, load};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "data.diff",
    summary: "Compare two documents by structure, across formats",
    details: &["Key order and formatting do not count. Paths use jq syntax."],
    input: Input::all(IN, 2, "data"),
    output: Output::report(),
    examples: &["deft data diff config.yaml override.yaml"],
    tags: &["compare json", "compare yaml", "what changed"],
    result: &["equal", "changes"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    if job.inputs.len() != 2 {
        return Err(err(ErrorCode::BadOption, "give exactly two files"));
    }
    let changes = deft_tools_data::diff(&load(&job.inputs[0])?, &load(&job.inputs[1])?);
    let list: Vec<_> = changes.iter().map(|c| c.to_json()).collect();
    let note = format!(
        "{} change{}",
        list.len(),
        if list.len() == 1 { "" } else { "s" }
    );
    Ok(OpResult::report(
        facts![("equal", list.is_empty()), ("changes", list)],
        note,
    ))
}
