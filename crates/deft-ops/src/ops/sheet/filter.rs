use super::_common::{IN, OUT, SHEET, TO, finish, input_table};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "sheet.filter",
    summary: "Keep matching rows: --where \"age > 30 and city = 'Oslo'\"",
    details: &[
        "Compare with = != < <= > >=, contains, startswith, endswith, in (a, b), ~ 'regex'.",
        "Join with and, or, not, parentheses. Also: col is empty, col is not empty.",
        "Text comparison ignores case. Quote names with spaces: `Unit price` > 3.",
    ],
    input: Input::each(IN, "sheet"),
    output: Output::one(OUT, "sheet"),
    params: &[
        param("where", Kind::Text, "Condition on columns").required(),
        SHEET,
        TO,
    ],
    examples: &[
        "deft sheet filter people.csv --where 'age > 30' -o older.csv",
        "deft sheet filter people.xlsx --where \"name in (Ada, Sam)\" --to csv -o some.csv",
    ],
    tags: &["where", "rows", "search", "subset", "query csv"],
    result: &["rows", "columns"],
    suffix: Some("filtered"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let t = input_table(job)?;
    let text = job.params.text("where");
    let f = deft_tools_sheet::parse_filter(&text, &t).map_err(|m| bad_value("where", m))?;
    finish(job, &deft_tools_sheet::filter(&t, &f))
}
