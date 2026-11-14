//! What the json, yaml, toml and xml groups share: each group has the same five
//! commands, and each command runs the same code on its own format.

use deft_core::prelude::*;
use serde_json::Value as Json;

pub const JSON: &[Format] = &[Format::Json, Format::Jsonl];
pub const YAML: &[Format] = &[Format::Yaml];
pub const TOML: &[Format] = &[Format::Toml];
pub const XML: &[Format] = &[Format::Xml];
pub const TO_ANY: &[Format] = &[
    Format::Json,
    Format::Jsonl,
    Format::Yaml,
    Format::Toml,
    Format::Xml,
];

pub const SYNONYMS: &[(&str, &str)] = &[
    ("pretty", "format"),
    ("prettify", "format"),
    ("beautify", "format"),
    ("lint", "validate"),
    ("check", "validate"),
    ("compare", "diff"),
    ("combine", "merge"),
];

pub const SORT_KEYS: ParamSpec = param("sort-keys", Kind::Flag, "Sort object keys");
pub const ARRAYS: ParamSpec = param(
    "arrays",
    Kind::Choice(&["replace", "append"]),
    "What to do with two lists at one key",
)
.default("replace");
pub const SCHEMA: ParamSpec = param("schema", Kind::Path, "JSON Schema file, in JSON or YAML");

fn load(input: &InputFile) -> Result<Json> {
    let format = input.require_format()?;
    deft_tools_data::parse(input.bytes()?, format).map_err(|mut e| {
        e.message = format!("{}: {}", input.display, e.message);
        e
    })
}

/// What reading these inputs dropped, to pass on as warnings.
fn lost(inputs: &[InputFile]) -> Vec<String> {
    let mut all = Vec::new();
    for input in inputs {
        if let (Some(format), Ok(bytes)) = (input.format, input.bytes()) {
            for w in deft_tools_data::lost_on_reading(bytes, format) {
                if !all.contains(&w) {
                    all.push(w);
                }
            }
        }
    }
    all
}

/// The output format: `--to`, then `-o name.ext`, then the input's format.
fn target(job: &Job) -> Format {
    let input = job.input().format.filter(|f| TO_ANY.contains(f));
    job.params
        .opt_text("to")
        .and_then(|t| Format::from_name(&t))
        .unwrap_or_else(|| job.target(input.unwrap_or(Format::Json)))
}

fn write(job: &Job, value: &Json, style: deft_tools_data::Style) -> Result<OpResult> {
    let format = target(job);
    let (bytes, warnings) = deft_tools_data::render(value, format, style)?;
    let mut res = OpResult::one(Produced::bytes(bytes, format));
    for w in lost(&job.inputs).into_iter().chain(warnings) {
        res = res.warn(w);
    }
    Ok(res)
}

pub fn convert(job: &Job) -> Result<OpResult> {
    if !job.params.has("to") && job.out_format.is_none() {
        return Err(err(
            ErrorCode::BadOption,
            "--to is required (or give -o name.ext)",
        ));
    }
    write(job, &load(job.input())?, Default::default())
}

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

pub fn format(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let style = deft_tools_data::Style {
        compact: p.flag("compact"),
        sort_keys: p.flag("sort-keys"),
        indent: p.opt_int("indent").unwrap_or(2) as usize,
    };
    let input = job.input();
    if input.format == Some(Format::Yaml) {
        let docs = deft_tools_data::yaml_documents(input.bytes()?)?;
        if docs.len() > 1 {
            return documents(job, &docs, style);
        }
    }
    write(job, &load(input)?, style)
}

pub fn merge(job: &Job) -> Result<OpResult> {
    let append = job.params.text("arrays") == "append";
    let mut all = job.inputs.iter().map(load);
    let mut value = all.next().expect("two inputs")?;
    for next in all {
        deft_tools_data::merge(&mut value, next?, append);
    }
    write(job, &value, Default::default())
}

pub fn diff(job: &Job) -> Result<OpResult> {
    if job.inputs.len() != 2 {
        return Err(err(ErrorCode::BadOption, "give exactly two files"));
    }
    let changes = deft_tools_data::diff(&load(&job.inputs[0])?, &load(&job.inputs[1])?);
    let list: Vec<_> = changes.iter().map(|c| c.to_json()).collect();
    let note = format!(
        "{} change{}",
        list.len(),
        if list.len() == 1 { "" } else { "s" }
    );
    Ok(OpResult::report(
        facts![("equal", list.is_empty()), ("changes", list)],
        note,
    ))
}

pub fn validate(job: &Job) -> Result<OpResult> {
    let value = load(job.input())?;
    let mut res_warnings = Vec::new();
    if let Some(path) = job.params.opt_path("schema") {
        let bytes = std::fs::read(&path)
            .map_err(|e| bad_value("schema", format!("{}: {e}", path.display())))?;
        let format = Format::from_path(&path).unwrap_or(Format::Json);
        let schema = deft_tools_data::parse(&bytes, format)?;
        let checked = deft_tools_data::validate_schema(&value, &schema, Some(&path))?;
        if !checked.unchecked.is_empty() {
            res_warnings.push(format!(
                "not checked: the parts of the schema that refer to {} (deft does not fetch schemas from the network)",
                checked.unchecked.join(", ")
            ));
        }
        if !checked.problems.is_empty() {
            let mut e = err(
                ErrorCode::InputCorrupt,
                format!("{}: {}", job.input().display, checked.problems.join("; ")),
            );
            if let Some(w) = res_warnings.iter().find(|w| w.starts_with("not checked")) {
                e = e.hint(w.clone());
            }
            return Err(e);
        }
    }
    let format = job.input().require_format()?;
    let mut res = OpResult::report(facts![("valid", true), ("format", format.label())], "");
    for w in res_warnings {
        res = res.warn(w);
    }
    Ok(res)
}
