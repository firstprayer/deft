use crate::structured::{TO_ANY, XML, convert};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "xml.convert",
    summary: "Change format: --to json|yaml|toml|jsonl",
    details: &[
        "Attributes become @name keys; repeated elements become lists.",
        "Text mixed with elements becomes a #content list in order; a plist <dict> becomes an object.",
    ],
    input: Input::each(XML, "xml"),
    output: Output::one(TO_ANY, "data"),
    params: &[param(
        "to",
        Kind::Choice(&["json", "yaml", "toml", "jsonl"]),
        "Target format (or give -o name.ext)",
    )],
    converts: &[Edge {
        from: XML,
        to: TO_ANY,
        quality: Quality::Faithful,
    }],
    examples: &["deft xml convert shop.xml --to json -o out.json"],
    tags: &["xml to json", "xml to yaml", "xml to toml"],
    run: convert,
    ..Op::DEFAULT
};
