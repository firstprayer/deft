use super::_common::{IN, input_table, shape, warned, write};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "sheet.to-md",
    summary: "Markdown table (first row is the header)",
    input: Input::each(IN, "sheet"),
    output: Output::one(&[Format::Md], "md"),
    params: &[param(
        "sheet",
        Kind::Text,
        "Sheet name or number (default: first visible)",
    )],
    converts: &[Edge {
        from: IN,
        to: &[Format::Md],
        quality: Quality::Faithful,
    }],
    examples: &[
        "deft sheet to-md people.csv -o people.md",
        "deft sheet to-md people.xlsx -o -",
    ],
    tags: &["markdown table", "preview", "read"],
    result: &["rows", "columns"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let (t, warning) = input_table(job)?;
    let res = OpResult::one(write(&t, Format::Md)?)
        .fact("rows", t.rows.len())
        .fact("columns", t.columns())
        .note(shape(&t));
    Ok(warned(res, warning))
}
