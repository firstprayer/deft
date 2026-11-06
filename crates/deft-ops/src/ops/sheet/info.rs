use super::_common::{IN, tables};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "sheet.info",
    summary: "Sheet names, rows, columns",
    input: Input::each(IN, "sheet"),
    output: Output::report(),
    examples: &["deft sheet info people.xlsx"],
    tags: &["rows", "columns", "sheets", "count"],
    result: &["sheets"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let t = tables(job.input())?;
    let sheets: Vec<serde_json::Value> = t
        .iter()
        .map(|s| serde_json::json!({"name": s.name, "rows": s.rows.len(), "columns": s.columns()}))
        .collect();
    let note = t
        .iter()
        .map(|s| format!("{}: {}", s.name, super::_common::shape(s)))
        .collect::<Vec<_>>()
        .join(", ");
    Ok(OpResult::report(facts![("sheets", sheets)], note))
}
