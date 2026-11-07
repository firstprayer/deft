use super::_common::{PASSWORD, open, write};
use deft_core::prelude::*;
use serde_json::Value as Json;
use std::collections::BTreeMap;

pub static OP: Op = Op {
    id: "pdf.fill",
    summary: "Fill a form from JSON: --data values.json",
    details: &[
        "The JSON is {\"field name\": value}. Checkboxes take true or false; radios an option;",
        "a multi-select list a list of options.",
        "Field names come from deft pdf fields; a short name works when it is unique.",
    ],
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::one(&[Format::Pdf], "pdf"),
    params: &[
        param("data", Kind::Path, "JSON file of field values").required(),
        param("flatten", Kind::Flag, "Make the result uneditable"),
        PASSWORD,
    ],
    examples: &[
        "deft pdf fill form.pdf --data form-values.json -o filled.pdf",
        "deft pdf fill form.pdf --data form-values.json --flatten -o final.pdf",
    ],
    tags: &["form", "acroform", "fill in", "tax form", "application"],
    result: &["filled"],
    errors: &[ErrorCode::BadValue],
    suffix: Some("filled"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let mut doc = open(job.input(), p)?;
    let path = p.path("data");
    let text =
        std::fs::read(&path).map_err(|e| bad_value("data", format!("{}: {e}", path.display())))?;
    let values: BTreeMap<String, Json> = serde_json::from_slice(&text)
        .map_err(|e| bad_value("data", format!("not a JSON object: {e}")))?;
    let warnings = deft_tools_pdf::forms::fill(&mut doc, &values)?;
    if p.flag("flatten") {
        deft_tools_pdf::forms::flatten(&mut doc)?;
    }
    let mut res = OpResult::one(write(&mut doc)?)
        .fact("filled", values.len())
        .note(format!("{} fields", values.len()));
    res.warnings = warnings;
    Ok(res)
}
