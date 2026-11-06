use super::_common::files;
use deft_core::prelude::*;
use deft_tools_file::RenameRule;

pub static OP: Op = Op {
    id: "file.rename",
    summary: "Batch rename: --find, --replace, --name, --case; --preview",
    details: &[
        "--find is a regex on the name without its extension; $1 works in --replace.",
        "--name is a template: {stem} {ext} {n} {n:03} {parent}. Files stay in their folder.",
        "Files are renamed in place. --preview shows the plan and changes nothing.",
    ],
    input: Input::any_files(1),
    output: Output::report(),
    params: &[
        param("find", Kind::Text, "Regex to replace in the name"),
        param("replace", Kind::Text, "Replacement for --find").default(""),
        param("case", Kind::Choice(&["upper", "lower"]), "Change the case"),
        param("name", Kind::Text, "Template for the whole name"),
        param(
            "start",
            Kind::Int {
                min: 0,
                max: 1_000_000_000,
            },
            "First {n}",
        )
        .default("1"),
        param("preview", Kind::Flag, "Print the plan only"),
    ],
    examples: &[
        "deft file rename photo-1.jpg photo-2.jpg --name 'trip-{n:02}.{ext}' --preview",
        "deft file rename notes.md --find '^notes$' --replace readme",
    ],
    tags: &[
        "rename files",
        "sequential numbers",
        "prefix",
        "lowercase names",
        "mass rename",
    ],
    result: &["renamed", "plan"],
    errors: &[ErrorCode::OutputExists],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let find = match p.opt_text("find") {
        Some(f) => Some(
            regex::Regex::new(&f).map_err(|e| bad_value("find", format!("bad pattern: {e}")))?,
        ),
        None => None,
    };
    if find.is_none() && p.opt_text("case").is_none() && p.opt_text("name").is_none() {
        return Err(err(ErrorCode::BadOption, "give --find, --case or --name"));
    }
    let rule = RenameRule {
        find,
        replace: p.text("replace"),
        case: p.opt_text("case"),
        name: p.opt_text("name"),
        start: p.int("start") as usize,
    };
    let plan = deft_tools_file::rename_plan(&files(job)?, &rule)?;
    let list: Vec<_> = plan
        .iter()
        .map(|(a, b)| {
            serde_json::json!({"from": a.display().to_string(), "to": b.display().to_string()})
        })
        .collect();
    let preview = p.flag("preview");
    if !preview {
        deft_tools_file::apply_renames(&plan)?;
    }
    let note = format!(
        "{} {}",
        if preview { "would rename" } else { "renamed" },
        plan.len()
    );
    Ok(OpResult::report(
        facts![
            ("renamed", if preview { 0 } else { plan.len() }),
            ("plan", list)
        ],
        note,
    ))
}
