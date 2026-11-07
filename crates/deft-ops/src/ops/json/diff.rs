use crate::structured::{JSON, diff};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "json.diff",
    summary: "Compare two files by structure",
    details: &["Key order and formatting do not count."],
    input: Input::all(JSON, 2, "json"),
    output: Output::report(),
    examples: &["deft json diff messy.json override.json"],
    tags: &["compare json", "what changed"],
    result: &["equal", "changes"],
    run: diff,
    ..Op::DEFAULT
};
