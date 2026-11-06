use super::_common::{IN, OUT, SHEET, TO, columns, finish, input_table};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "sheet.dedupe",
    summary: "Drop repeated rows, by whole row or --columns",
    input: Input::each(IN, "sheet"),
    output: Output::one(OUT, "sheet"),
    params: &[
        param("columns", Kind::Text, "Compare only these columns"),
        param(
            "keep",
            Kind::Choice(&["first", "last"]),
            "Which repeat stays",
        )
        .default("first"),
        SHEET,
        TO,
    ],
    examples: &[
        "deft sheet dedupe people.csv -o unique.csv",
        "deft sheet dedupe people.csv --columns name --keep last -o latest.csv",
    ],
    tags: &["unique", "distinct", "remove duplicates", "duplicate rows"],
    result: &["rows", "columns", "removed"],
    suffix: Some("deduped"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let t = input_table(job)?;
    let cols = match p.opt_text("columns") {
        Some(list) => Some(columns(&t, "columns", &list)?),
        None => None,
    };
    let out = deft_tools_sheet::dedupe(&t, cols.as_deref(), p.text("keep") == "last");
    let removed = t.rows.len() - out.rows.len();
    Ok(finish(job, &out)?.fact("removed", removed).note(format!(
        "{removed} removed, {} rows left",
        out.rows.len().saturating_sub(1)
    )))
}
