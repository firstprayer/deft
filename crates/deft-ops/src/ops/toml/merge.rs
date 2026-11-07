use crate::structured::{ARRAYS, TOML, merge};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "toml.merge",
    summary: "Deep-merge files; later files win",
    input: Input::all(TOML, 2, "toml"),
    output: Output::one(&[Format::Toml], "toml"),
    params: &[ARRAYS],
    examples: &["deft toml merge settings.toml override.toml -o merged.toml"],
    tags: &["combine toml", "merge toml", "override config", "overlay"],
    suffix: Some("merged"),
    run: merge,
    ..Op::DEFAULT
};
