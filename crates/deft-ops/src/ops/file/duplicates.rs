use super::_common::files;
use deft_core::prelude::*;
use serde_json::json;

pub static OP: Op = Op {
    id: "file.duplicates",
    summary: "Find files with identical content; deletes nothing",
    details: &["Compares content, not names. Empty files and hidden entries are skipped."],
    input: Input::any_files(1),
    output: Output::report(),
    examples: &["deft file duplicates . --json"],
    tags: &[
        "dedupe",
        "same files",
        "duplicate photos",
        "fdupes",
        "wasted space",
    ],
    result: &["groups", "wasted_bytes"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let groups = deft_tools_file::duplicates(&files(job)?)?;
    let wasted: u64 = groups
        .iter()
        .map(|g| g.size * (g.files.len() as u64 - 1))
        .sum();
    let list: Vec<_> = groups
        .iter()
        .map(|g| {
            let files: Vec<String> = g.files.iter().map(|f| f.display().to_string()).collect();
            json!({"bytes": g.size, "files": files})
        })
        .collect();
    let note = format!("{} groups, {} wasted", list.len(), format_bytes(wasted));
    Ok(OpResult::report(
        facts![("groups", list), ("wasted_bytes", wasted)],
        note,
    ))
}
