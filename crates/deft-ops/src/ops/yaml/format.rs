use crate::structured::{SORT_KEYS, YAML, format};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "yaml.format",
    summary: "Tidy a file: --sort-keys",
    input: Input::each(YAML, "yaml"),
    output: Output::one(&[Format::Yaml], "yaml"),
    params: &[SORT_KEYS],
    examples: &["deft yaml format config.yaml -o tidy.yaml"],
    tags: &["tidy yaml", "sort keys"],
    suffix: Some("formatted"),
    run: format,
    ..Op::DEFAULT
};
