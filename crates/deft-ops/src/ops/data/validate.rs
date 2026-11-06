use super::_common::{IN, load};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "data.validate",
    summary: "Check syntax, or a JSON Schema with --schema",
    input: Input::each(IN, "data"),
    output: Output::report(),
    params: &[param("schema", Kind::Path, "JSON Schema file")],
    examples: &[
        "deft data validate config.yaml settings.toml",
        "deft data validate config.yaml --schema server.schema.json",
    ],
    tags: &["lint json", "lint yaml", "syntax check", "is valid"],
    result: &["valid", "format"],
    errors: &[ErrorCode::InputCorrupt],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let value = load(job.input())?;
    if let Some(path) = job.params.opt_path("schema") {
        let bytes = std::fs::read(&path)
            .map_err(|e| bad_value("schema", format!("{}: {e}", path.display())))?;
        let schema = deft_tools_data::parse(&bytes, Format::Json)?;
        let problems = deft_tools_data::validate_schema(&value, &schema)?;
        if !problems.is_empty() {
            return Err(err(
                ErrorCode::InputCorrupt,
                format!("{}: {}", job.input().display, problems.join("; ")),
            ));
        }
    }
    let format = job.input().require_format()?;
    Ok(OpResult::report(
        facts![("valid", true), ("format", format.label())],
        "",
    ))
}
