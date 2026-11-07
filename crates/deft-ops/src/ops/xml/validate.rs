use crate::structured::{SCHEMA, XML, validate};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "xml.validate",
    summary: "Check syntax, or a JSON Schema with --schema",
    input: Input::each(XML, "xml"),
    output: Output::report(),
    params: &[SCHEMA],
    examples: &[
        "deft xml validate shop.xml",
        "deft xml validate shop.xml --schema object.schema.json",
    ],
    tags: &["lint xml", "syntax check", "is valid"],
    result: &["valid", "format"],
    errors: &[ErrorCode::InputCorrupt],
    run: validate,
    ..Op::DEFAULT
};
