use super::_common::{IN, OUT, pick, shape, tables, write};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "sheet.extract",
    summary: "Take a sheet or range: --sheet, --range A1:D20",
    input: Input::each(IN, "sheet"),
    output: Output::one(OUT, "sheet"),
    params: &[
        param("sheet", Kind::Text, "Sheet name or number (default: first)"),
        param("range", Kind::Text, "Cell range, e.g. B2:D10"),
        param(
            "to",
            Kind::Choice(&["csv", "xlsx", "json"]),
            "Output format",
        )
        .default("csv"),
    ],
    examples: &[
        "deft sheet extract people.xlsx --range A1:B2 -o part.csv",
        "deft sheet extract people.csv --range A1:A3 --to json -o names.json",
    ],
    tags: &["cells", "subset", "columns", "select"],
    result: &["rows", "columns"],
    suffix: Some("extract"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let mut t = pick(tables(job.input())?, p.opt_text("sheet"))?;
    if let Some(r) = p.opt_text("range") {
        let range = deft_tools_sheet::parse_range(&r)
            .ok_or_else(|| bad_value("range", format!("{r:?} is not a range like A1:D20")))?;
        t = deft_tools_sheet::slice(&t, range);
    }
    let format = Format::from_name(&p.text("to")).expect("choice");
    Ok(OpResult::one(write(&t, format)?)
        .fact("rows", t.rows.len())
        .fact("columns", t.columns())
        .note(shape(&t)))
}
