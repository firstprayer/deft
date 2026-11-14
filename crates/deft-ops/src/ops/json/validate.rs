use crate::structured::{JSON, SCHEMA, validate};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "json.validate",
    summary: "Check syntax, or a JSON Schema with --schema",
    details: &["Schemas referred to by URL are not fetched; that part is reported as not checked."],
    input: Input::each(JSON, "json"),
    output: Output::report(),
    params: &[SCHEMA],
    examples: &[
        "deft json validate messy.json",
        "deft json validate messy.json --schema object.schema.json",
    ],
    tags: &["lint json", "syntax check", "is valid"],
    result: &["valid", "format"],
    errors: &[ErrorCode::InputCorrupt],
    run: validate,
    ..Op::DEFAULT
};
