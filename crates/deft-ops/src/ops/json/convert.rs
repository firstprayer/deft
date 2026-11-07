use crate::structured::{JSON, TO_ANY, convert};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "json.convert",
    summary: "Change format: --to json|jsonl|yaml|toml|xml",
    input: Input::each(JSON, "json"),
    output: Output::one(TO_ANY, "data"),
    params: &[param(
        "to",
        Kind::Choice(&["json", "jsonl", "yaml", "toml", "xml"]),
        "Target format (or give -o name.ext)",
    )],
    converts: &[Edge {
        from: JSON,
        to: TO_ANY,
        quality: Quality::Faithful,
    }],
    examples: &["deft json convert messy.json --to yaml -o out.yaml"],
    tags: &["json to yaml", "json to toml", "json to xml", "ndjson"],
    run: convert,
    ..Op::DEFAULT
};
