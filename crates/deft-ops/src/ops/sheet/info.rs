use super::_common::{IN, tables};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "sheet.info",
    summary: "Sheet names, rows, columns",
    details: &["Hidden sheets are marked hidden."],
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
        .map(|s| {
            let mut v =
                serde_json::json!({"name": s.name, "rows": s.rows.len(), "columns": s.columns()});
            if s.hidden {
                v["hidden"] = serde_json::json!(true);
            }
            v
        })
        .collect();
    let note = t
        .iter()
        .map(|s| {
            let hidden = if s.hidden { " (hidden)" } else { "" };
            format!("{}{hidden}: {}", s.name, super::_common::shape(s))
        })
        .collect::<Vec<_>>()
        .join(", ");
    let mut res = OpResult::report(facts![("sheets", sheets)], note);
    for w in t.iter().flat_map(|s| s.notes.iter()) {
        res = res.warn(w.clone());
    }
    Ok(res)
}
