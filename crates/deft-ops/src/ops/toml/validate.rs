use crate::structured::{SCHEMA, TOML, validate};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "toml.validate",
    summary: "Check syntax, or a JSON Schema with --schema",
    details: &["Schemas referred to by URL are not fetched; that part is reported as not checked."],
    input: Input::each(TOML, "toml"),
    output: Output::report(),
    params: &[SCHEMA],
    examples: &[
        "deft toml validate settings.toml",
        "deft toml validate settings.toml --schema object.schema.json",
    ],
    tags: &["lint toml", "syntax check", "is valid"],
    result: &["valid", "format"],
    errors: &[ErrorCode::InputCorrupt],
    run: validate,
    ..Op::DEFAULT
};
