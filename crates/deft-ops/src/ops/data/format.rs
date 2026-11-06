use super::_common::{IN, OUT, load, target, write};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "data.format",
    summary: "Pretty-print or minify: --compact, --sort-keys",
    input: Input::each(IN, "data"),
    output: Output::one(OUT, "data"),
    params: &[
        param("compact", Kind::Flag, "JSON on one line"),
        param("sort-keys", Kind::Flag, "Sort object keys"),
        param(
            "indent",
            Kind::Int { min: 1, max: 16 },
            "Spaces per level for JSON",
        )
        .default("2"),
    ],
    examples: &[
        "deft data format messy.json -o tidy.json",
        "deft data format messy.json --compact --sort-keys -o -",
    ],
    tags: &[
        "pretty print json",
        "minify json",
        "beautify",
        "indent",
        "sort keys",
    ],
    suffix: Some("formatted"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let style = deft_tools_data::Style {
        compact: p.flag("compact"),
        sort_keys: p.flag("sort-keys"),
        indent: p.int("indent") as usize,
    };
    write(&load(job.input())?, target(job), style)
}
