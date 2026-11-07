use crate::structured::{XML, diff};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "xml.diff",
    summary: "Compare two files by structure",
    details: &["Key order and formatting do not count."],
    input: Input::all(XML, 2, "xml"),
    output: Output::report(),
    examples: &["deft xml diff shop.xml override.xml"],
    tags: &["compare xml", "what changed"],
    result: &["equal", "changes"],
    run: diff,
    ..Op::DEFAULT
};
