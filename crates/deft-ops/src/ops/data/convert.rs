use super::_common::{IN, OUT, load, target, write};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "data.convert",
    summary: "Change format: --to json|yaml|toml|xml|jsonl",
    details: &["XML attributes become @name keys; repeated elements become lists."],
    input: Input::each(IN, "data"),
    output: Output::one(OUT, "data"),
    params: &[param(
        "to",
        Kind::Choice(&["json", "yaml", "toml", "xml", "jsonl"]),
        "Target format (or give -o name.ext)",
    )],
    converts: &[Edge {
        from: IN,
        to: OUT,
        quality: Quality::Faithful,
    }],
    examples: &[
        "deft data convert config.yaml -o settings.json",
        "deft data convert settings.toml --to yaml -o settings.yaml",
    ],
    tags: &[
        "json to yaml",
        "yaml to json",
        "toml",
        "xml to json",
        "ndjson",
    ],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    if !job.params.has("to") && job.out_format.is_none() {
        return Err(err(
            ErrorCode::BadOption,
            "--to is required (or give -o name.ext)",
        ));
    }
    write(&load(job.input())?, target(job), Default::default())
}
