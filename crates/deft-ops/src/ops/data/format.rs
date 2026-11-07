use super::_common::{IN, OUT, load, lost, target, write};
use deft_core::prelude::*;
use serde_json::Value as Json;

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

/// A YAML file of several documents stays several documents.
fn documents(job: &Job, docs: &[Json], style: deft_tools_data::Style) -> Result<OpResult> {
    let mut text = Vec::new();
    for (i, doc) in docs.iter().enumerate() {
        if i > 0 {
            text.extend_from_slice(b"---\n");
        }
        text.extend(deft_tools_data::render(doc, Format::Yaml, style)?.0);
    }
    let mut res = OpResult::one(Produced::bytes(text, Format::Yaml));
    for w in lost(&job.inputs) {
        res = res.warn(w);
    }
    Ok(res)
}

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let style = deft_tools_data::Style {
        compact: p.flag("compact"),
        sort_keys: p.flag("sort-keys"),
        indent: p.int("indent") as usize,
    };
    let input = job.input();
    if input.format == Some(Format::Yaml) && target(job) == Format::Yaml {
        let docs = deft_tools_data::yaml_documents(input.bytes()?)?;
        if docs.len() > 1 {
            return documents(job, &docs, style);
        }
    }
    write(job, &load(input)?, style)
}
