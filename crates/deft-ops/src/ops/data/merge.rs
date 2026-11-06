use super::_common::{IN, OUT, load, target, write};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "data.merge",
    summary: "Deep-merge documents; later files win",
    input: Input::all(IN, 2, "data"),
    output: Output::one(OUT, "data"),
    params: &[
        param(
            "arrays",
            Kind::Choice(&["replace", "append"]),
            "What to do with two lists at one key",
        )
        .default("replace"),
        param(
            "to",
            Kind::Choice(&["json", "yaml", "toml", "xml", "jsonl"]),
            "Target format (or give -o name.ext)",
        ),
    ],
    examples: &[
        "deft data merge config.yaml override.yaml -o merged.yaml",
        "deft data merge config.yaml override.yaml --arrays append --to json -o merged.json",
    ],
    tags: &["combine json", "override config", "overlay"],
    suffix: Some("merged"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let append = job.params.text("arrays") == "append";
    let mut all = job.inputs.iter().map(load);
    let mut value = all.next().expect("two inputs")?;
    for next in all {
        deft_tools_data::merge(&mut value, next?, append);
    }
    write(&value, target(job), Default::default())
}
