use super::_common::{IN, OUT, SHEET, TO, columns, finish, input_table};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "sheet.sort",
    summary: "Sort rows: --columns age,name --descending",
    details: &["Numbers sort as numbers; empty cells go last."],
    input: Input::each(IN, "sheet"),
    output: Output::one(OUT, "sheet"),
    params: &[
        param(
            "columns",
            Kind::Text,
            "Column names or numbers, comma-separated",
        )
        .required(),
        param("descending", Kind::Flag, "Largest first"),
        SHEET,
        TO,
    ],
    examples: &["deft sheet sort people.csv --columns age --descending -o ranked.csv"],
    tags: &["order", "rank", "arrange", "top"],
    result: &["rows", "columns"],
    suffix: Some("sorted"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let t = input_table(job)?;
    let cols = columns(&t, "columns", &job.params.text("columns"))?;
    finish(
        job,
        &deft_tools_sheet::sort(&t, &cols, job.params.flag("descending")),
    )
}
