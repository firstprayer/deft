use super::_common::{IN, OUT, SHEET, TO, column, finish, input_table};
use deft_core::prelude::*;
use deft_tools_sheet::Agg;

pub static OP: Op = Op {
    id: "sheet.pivot",
    summary: "Pivot table: --rows city --columns year --values sales",
    details: &["Without --values, counts rows."],
    input: Input::each(IN, "sheet"),
    output: Output::one(OUT, "sheet"),
    params: &[
        param("rows", Kind::Text, "Column whose values become rows").required(),
        param("columns", Kind::Text, "Column whose values become columns").required(),
        param("values", Kind::Text, "Column to aggregate"),
        param(
            "agg",
            Kind::Choice(&["sum", "mean", "count", "min", "max"]),
            "How to combine values",
        )
        .default("sum"),
        SHEET,
        TO,
    ],
    examples: &["deft sheet pivot sales.csv --rows city --columns year --values amount -o p.csv"],
    tags: &["crosstab", "cross tab", "reshape", "wide", "summary table"],
    result: &["rows", "columns"],
    suffix: Some("pivot"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let t = input_table(job)?;
    let row = column(&t, "rows", &p.text("rows"))?;
    let col = column(&t, "columns", &p.text("columns"))?;
    let value = match p.opt_text("values") {
        Some(v) => Some(column(&t, "values", &v)?),
        None => None,
    };
    let agg = Agg::parse(&p.text("agg"));
    finish(job, &deft_tools_sheet::pivot(&t, row, col, value, agg))
}
