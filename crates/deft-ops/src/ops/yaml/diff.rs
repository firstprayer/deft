use crate::structured::{YAML, diff};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "yaml.diff",
    summary: "Compare two files by structure",
    details: &["Key order and formatting do not count."],
    input: Input::all(YAML, 2, "yaml"),
    output: Output::report(),
    examples: &["deft yaml diff config.yaml override.yaml"],
    tags: &["compare yaml", "what changed"],
    result: &["equal", "changes"],
    run: diff,
    ..Op::DEFAULT
};
