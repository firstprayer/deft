use crate::structured::{TOML, diff};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "toml.diff",
    summary: "Compare two files by structure",
    details: &["Key order and formatting do not count."],
    input: Input::all(TOML, 2, "toml"),
    output: Output::report(),
    examples: &["deft toml diff settings.toml override.toml"],
    tags: &["compare toml", "what changed"],
    result: &["equal", "changes"],
    run: diff,
    ..Op::DEFAULT
};
