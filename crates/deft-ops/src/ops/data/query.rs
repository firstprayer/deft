use super::_common::{IN, OUT, load, write};
use deft_core::prelude::*;
use serde_json::Value as Json;

const WRITES: &[Format] = &[
    Format::Json,
    Format::Jsonl,
    Format::Yaml,
    Format::Toml,
    Format::Xml,
    Format::Txt,
    Format::Md,
    Format::Html,
    Format::Csv,
];

pub static OP: Op = Op {
    id: "data.query",
    summary: "Query or edit with jq syntax: --expr '.items[].name'",
    details: &[
        "Works on JSON, YAML, TOML, XML and CSV. Several results become a list.",
        "--raw, or a name such as out.txt, gives plain text, one result per line.",
    ],
    input: Input::each(IN, "data"),
    output: Output::one(WRITES, "data"),
    params: &[
        param("expr", Kind::Text, "jq program").required(),
        param("raw", Kind::Flag, "Plain text, one result per line"),
    ],
    examples: &[
        "deft data query config.yaml --expr .server.port -o -",
        "deft data query people.csv --expr '.[] | select(.age > 30) | .name' --raw -o -",
        "deft data query config.yaml --expr '.server.port = 9090' -o changed.yaml",
    ],
    tags: &[
        "jq",
        "yq",
        "jsonpath",
        "extract field",
        "select",
        "update value",
    ],
    result: &["results"],
    suffix: Some("query"),
    run,
    ..Op::DEFAULT
};

fn line(v: &Json) -> String {
    match v {
        Json::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let value = load(job.input())?;
    let mut results = deft_tools_data::query(&value, &p.text("expr"))?;
    let count = results.len();
    let format = job.target(
        job.input()
            .format
            .filter(|f| OUT.contains(f))
            .unwrap_or(Format::Json),
    );
    if p.flag("raw") || !OUT.contains(&format) {
        let mut text = results.iter().map(line).collect::<Vec<_>>().join("\n");
        text.push('\n');
        let format = if OUT.contains(&format) {
            Format::Txt
        } else {
            format
        };
        return Ok(OpResult::one(Produced::bytes(text.into_bytes(), format)).fact("results", count));
    }
    let doc = if count == 1 {
        results.remove(0)
    } else {
        Json::Array(results)
    };
    Ok(write(&doc, format, Default::default())?.fact("results", count))
}
