use super::_common::{IN, OUT, SHEET, TO, finish, input_table};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "sheet.transpose",
    summary: "Swap rows and columns",
    input: Input::each(IN, "sheet"),
    output: Output::one(OUT, "sheet"),
    params: &[SHEET, TO],
    examples: &["deft sheet transpose people.csv -o turned.csv"],
    tags: &["flip table", "rows to columns"],
    result: &["rows", "columns"],
    suffix: Some("transposed"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    finish(job, &deft_tools_sheet::transpose(&input_table(job)?))
}
