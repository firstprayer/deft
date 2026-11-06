use super::_common::{IN, OUT, TO, column, finish, tables};
use deft_core::prelude::*;
use deft_tools_sheet::Join;

pub static OP: Op = Op {
    id: "sheet.join",
    summary: "Join two tables on a key: --on id --how left",
    details: &["Different key names: --on left_name=right_name. Use once per pair of files."],
    input: Input::all(IN, 2, "sheet"),
    output: Output::one(OUT, "sheet"),
    params: &[
        param("on", Kind::Text, "Key column, or left=right").required(),
        param(
            "how",
            Kind::Choice(&["inner", "left", "right", "outer"]),
            "Which unmatched rows stay",
        )
        .default("inner"),
        TO,
    ],
    examples: &[
        "deft sheet join people.csv cities.csv --on name -o joined.csv",
        "deft sheet join people.csv cities.csv --on name=name --how left -o all.csv",
    ],
    tags: &[
        "vlookup",
        "lookup",
        "merge columns",
        "combine tables",
        "sql join",
    ],
    result: &["rows", "columns"],
    suffix: Some("joined"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    if job.inputs.len() != 2 {
        return Err(err(ErrorCode::BadOption, "give exactly two tables"));
    }
    let p = job.params;
    let left = tables(&job.inputs[0])?.remove(0);
    let right = tables(&job.inputs[1])?.remove(0);
    let on = p.text("on");
    let (a, b) = on.split_once('=').unwrap_or((&on, &on));
    let how = match p.text("how").as_str() {
        "left" => Join::Left,
        "right" => Join::Right,
        "outer" => Join::Outer,
        _ => Join::Inner,
    };
    let (lk, rk) = (column(&left, "on", a)?, column(&right, "on", b)?);
    finish(job, &deft_tools_sheet::join(&left, &right, lk, rk, how))
}
