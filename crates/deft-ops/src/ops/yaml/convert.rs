use crate::structured::{TO_ANY, YAML, convert};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "yaml.convert",
    summary: "Change format: --to json|toml|xml|jsonl",
    input: Input::each(YAML, "yaml"),
    output: Output::one(TO_ANY, "data"),
    params: &[param(
        "to",
        Kind::Choice(&["json", "toml", "xml", "jsonl"]),
        "Target format (or give -o name.ext)",
    )],
    converts: &[Edge {
        from: YAML,
        to: TO_ANY,
        quality: Quality::Faithful,
    }],
    examples: &["deft yaml convert config.yaml --to json -o out.json"],
    tags: &["yaml to json", "yaml to toml", "yaml to xml"],
    run: convert,
    ..Op::DEFAULT
};
