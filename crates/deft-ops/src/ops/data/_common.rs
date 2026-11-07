//! Helpers shared by the data operations.

use deft_core::prelude::*;
use serde_json::Value as Json;

pub const IN: &[Format] = &[
    Format::Json,
    Format::Jsonl,
    Format::Yaml,
    Format::Toml,
    Format::Xml,
    Format::Csv,
];
pub const OUT: &[Format] = &[
    Format::Json,
    Format::Jsonl,
    Format::Yaml,
    Format::Toml,
    Format::Xml,
];

/// Read a document; CSV arrives as a list of objects keyed by the header row.
pub fn load(input: &InputFile) -> Result<Json> {
    let format = input.require_format()?;
    let bytes = input.bytes()?;
    if format == Format::Csv {
        let table = deft_tools_sheet::read_csv(bytes, &input.stem())?;
        let json = deft_tools_sheet::write_json(&table)?;
        return deft_tools_data::parse(&json, Format::Json);
    }
    deft_tools_data::parse(bytes, format).map_err(|mut e| {
        e.message = format!("{}: {}", input.display, e.message);
        e
    })
}

/// What reading these inputs dropped, to pass on as warnings.
pub fn lost(inputs: &[InputFile]) -> Vec<String> {
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
pub fn target(job: &Job) -> Format {
    let input = job.input().format.filter(|f| OUT.contains(f));
    job.params
        .opt_text("to")
        .and_then(|t| Format::from_name(&t))
        .unwrap_or_else(|| job.target(input.unwrap_or(Format::Json)))
}

pub fn write(job: &Job, value: &Json, style: deft_tools_data::Style) -> Result<OpResult> {
    let format = target(job);
    let (bytes, warnings) = deft_tools_data::render(value, format, style)?;
    let mut res = OpResult::one(Produced::bytes(bytes, format));
    for w in lost(&job.inputs).into_iter().chain(warnings) {
        res = res.warn(w);
    }
    Ok(res)
}
