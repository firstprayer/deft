use crate::structured::{SORT_KEYS, TOML, format};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "toml.format",
    summary: "Tidy a file: --sort-keys",
    input: Input::each(TOML, "toml"),
    output: Output::one(&[Format::Toml], "toml"),
    params: &[SORT_KEYS],
    examples: &["deft toml format settings.toml -o tidy.toml"],
    tags: &["tidy toml", "sort keys"],
    suffix: Some("formatted"),
    run: format,
    ..Op::DEFAULT
};
