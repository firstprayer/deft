use super::_common::{IN, OUT, SHEET, TO, columns, finish, input_table};
use deft_core::prelude::*;
use deft_tools_sheet::Agg;

pub static OP: Op = Op {
    id: "sheet.group",
    summary: "Group and total: --columns city --values sales --agg sum",
    details: &["Without --values, counts the rows of each group."],
    input: Input::each(IN, "sheet"),
    output: Output::one(OUT, "sheet"),
    params: &[
        param("columns", Kind::Text, "Columns to group by").required(),
        param("values", Kind::Text, "Columns to aggregate"),
        param(
            "agg",
            Kind::Choice(&["sum", "mean", "count", "min", "max"]),
            "How to combine values",
        )
        .default("sum"),
        SHEET,
        TO,
    ],
    examples: &[
        "deft sheet group sales.csv --columns city --values amount --agg mean -o totals.csv",
        "deft sheet group sales.csv --columns city,year -o counts.csv",
    ],
    tags: &["group by", "aggregate", "subtotal", "count per", "sum per"],
    result: &["rows", "columns"],
    suffix: Some("grouped"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let t = input_table(job)?;
    let by = columns(&t, "columns", &p.text("columns"))?;
    let values = match p.opt_text("values") {
        Some(v) => columns(&t, "values", &v)?,
        None => vec![],
    };
    finish(
        job,
        &deft_tools_sheet::group(&t, &by, &values, Agg::parse(&p.text("agg"))),
    )
}
