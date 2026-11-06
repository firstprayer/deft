use super::_common::{IN, SHEET, input_table};
use deft_core::prelude::*;
use serde_json::json;

pub static OP: Op = Op {
    id: "sheet.stats",
    summary: "Per-column counts, empties, min, max, mean, sum",
    input: Input::each(IN, "sheet"),
    output: Output::report(),
    params: &[SHEET],
    examples: &["deft sheet stats people.csv"],
    tags: &[
        "describe",
        "summary",
        "profile",
        "missing values",
        "average",
    ],
    result: &["rows", "columns"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let t = input_table(job)?;
    let columns: Vec<_> = deft_tools_sheet::stats(&t)
        .into_iter()
        .map(|c| {
            json!({"name": c.name, "filled": c.filled, "empty": c.empty, "unique": c.unique,
                   "min": c.min, "max": c.max, "mean": c.mean, "sum": c.sum})
        })
        .collect();
    Ok(OpResult::report(
        facts![
            ("rows", t.rows.len().saturating_sub(1)),
            ("columns", columns)
        ],
        "",
    ))
}
