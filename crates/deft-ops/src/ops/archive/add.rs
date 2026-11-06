use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "archive.add",
    summary: "Add files to a copy of a ZIP",
    details: &["The first input is the ZIP. Its entries are copied as they are."],
    input: Input::any_files(2),
    output: Output::one(&[Format::Zip], "zip"),
    examples: &["deft archive add bundle.zip report.pdf -o bigger.zip"],
    tags: &["append to zip", "update zip", "insert"],
    result: &["added"],
    suffix: Some("added"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let zip = job.input();
    if zip.format != Some(Format::Zip) {
        return Err(err(
            ErrorCode::InputMismatch,
            format!("{}: the first input must be a ZIP", zip.display),
        ));
    }
    let source = zip.local_path(job.ctx)?;
    let rest: Vec<_> = job.inputs[1..]
        .iter()
        .filter_map(|i| i.path.clone())
        .collect();
    let out = job.ctx.temp_path("out.zip");
    let n = deft_tools_archive::append_zip(&source, &out, &rest)?;
    Ok(OpResult::one(Produced::file(out, Some(Format::Zip)))
        .fact("added", n)
        .note(format!("{n} added")))
}
