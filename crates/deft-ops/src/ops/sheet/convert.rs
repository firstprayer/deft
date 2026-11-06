use super::_common::{IN, OUT, pick, shape, tables, write};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "sheet.convert",
    summary: "Change format: --to csv|xlsx|json",
    details: &["JSON is an array of objects keyed by the first row."],
    input: Input::each(IN, "sheet"),
    output: Output::one(OUT, "sheet"),
    params: &[
        param(
            "to",
            Kind::Choice(&["csv", "xlsx", "json"]),
            "Target format (or give -o name.ext)",
        ),
        param("sheet", Kind::Text, "Sheet name or number (default: first)"),
    ],
    converts: &[Edge {
        from: IN,
        to: OUT,
        quality: Quality::Faithful,
    }],
    examples: &[
        "deft sheet convert people.csv --to xlsx -o table.xlsx",
        "deft sheet convert people.xlsx --to json -o people.json",
    ],
    tags: &["csv to excel", "excel to csv", "xlsx", "json", "export"],
    result: &["rows", "columns"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let to = p.opt_text("to").ok_or_else(|| {
        err(
            ErrorCode::BadOption,
            "--to is required (or give -o name.ext)",
        )
    })?;
    let format = Format::from_name(&to).expect("choice");
    let t = pick(tables(job.input())?, p.opt_text("sheet"))?;
    Ok(OpResult::one(write(&t, format)?)
        .fact("rows", t.rows.len())
        .fact("columns", t.columns())
        .note(shape(&t)))
}
