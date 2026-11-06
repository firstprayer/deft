use super::_common::{IN, OUT, SHEET, TO, columns, finish, input_table};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "sheet.select",
    summary: "Keep, reorder or --drop columns",
    input: Input::each(IN, "sheet"),
    output: Output::one(OUT, "sheet"),
    params: &[
        param("columns", Kind::Text, "Columns to keep, in this order"),
        param("drop", Kind::Text, "Columns to remove"),
        SHEET,
        TO,
    ],
    examples: &[
        "deft sheet select people.csv --columns age,name -o swapped.csv",
        "deft sheet select people.csv --drop age -o names.csv",
    ],
    tags: &["pick columns", "remove column", "reorder columns", "cut"],
    result: &["rows", "columns"],
    suffix: Some("selected"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let mut t = input_table(job)?;
    if p.opt_text("columns").is_none() && p.opt_text("drop").is_none() {
        return Err(err(ErrorCode::BadOption, "give --columns or --drop"));
    }
    if let Some(list) = p.opt_text("columns") {
        t = deft_tools_sheet::select(&t, &columns(&t, "columns", &list)?);
    }
    if let Some(list) = p.opt_text("drop") {
        t = deft_tools_sheet::drop_columns(&t, &columns(&t, "drop", &list)?);
    }
    finish(job, &t)
}
