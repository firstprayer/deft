use crate::structured::{SCHEMA, YAML, validate};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "yaml.validate",
    summary: "Check syntax, or a JSON Schema with --schema",
    input: Input::each(YAML, "yaml"),
    output: Output::report(),
    params: &[SCHEMA],
    examples: &[
        "deft yaml validate config.yaml",
        "deft yaml validate config.yaml --schema server.schema.json",
    ],
    tags: &["lint yaml", "syntax check", "is valid"],
    result: &["valid", "format"],
    errors: &[ErrorCode::InputCorrupt],
    run: validate,
    ..Op::DEFAULT
};
