use crate::structured::{ARRAYS, JSON, merge};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "json.merge",
    summary: "Deep-merge files; later files win",
    input: Input::all(JSON, 2, "json"),
    output: Output::one(&[Format::Json], "json"),
    params: &[ARRAYS],
    examples: &["deft json merge messy.json override.json -o merged.json"],
    tags: &["combine json", "merge json", "override config", "overlay"],
    suffix: Some("merged"),
    run: merge,
    ..Op::DEFAULT
};
