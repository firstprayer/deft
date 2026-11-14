use super::_common::{IN, OUT, other_sheets, shape, tables, write};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "sheet.merge",
    summary: "Stack tables; a shared header is kept once",
    input: Input::all(IN, 2, "sheet"),
    output: Output::one(OUT, "sheet"),
    params: &[param(
        "to",
        Kind::Choice(&["csv", "xlsx", "json"]),
        "Output format",
    )
    .default("csv")],
    examples: &["deft sheet merge people.csv people2.csv -o all.csv"],
    tags: &["append", "combine", "concatenate", "stack", "union"],
    result: &["rows", "columns"],
    suffix: Some("merged"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let mut all = Vec::new();
    let mut warnings = Vec::new();
    for input in &job.inputs {
        let mut sheets = tables(input)?;
        warnings.extend(other_sheets(&sheets, None));
        let t = sheets.remove(0);
        warnings.extend(t.notes.iter().cloned());
        all.push(t);
    }
    let (t, unrelated) = deft_tools_sheet::concat(&all);
    let format = Format::from_name(&job.params.text("to")).expect("choice");
    let mut res = OpResult::one(write(&t, format)?)
        .fact("rows", t.rows.len())
        .fact("columns", t.columns())
        .note(shape(&t));
    if unrelated {
        res = res.warn("the files share no column names, so rows were stacked as they are");
    }
    for w in warnings {
        res = res.warn(w);
    }
    Ok(res)
}
