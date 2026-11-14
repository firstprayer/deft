use crate::structured::{TO_ANY, TOML, convert};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "toml.convert",
    summary: "Change format: --to json|yaml|xml|jsonl",
    details: &["Comments are kept in YAML."],
    input: Input::each(TOML, "toml"),
    output: Output::one(TO_ANY, "data"),
    params: &[param(
        "to",
        Kind::Choice(&["json", "yaml", "xml", "jsonl"]),
        "Target format (or give -o name.ext)",
    )],
    converts: &[Edge {
        from: TOML,
        to: TO_ANY,
        quality: Quality::Faithful,
    }],
    examples: &["deft toml convert settings.toml --to json -o out.json"],
    tags: &["toml to json", "toml to yaml", "toml to xml"],
    run: convert,
    ..Op::DEFAULT
};
