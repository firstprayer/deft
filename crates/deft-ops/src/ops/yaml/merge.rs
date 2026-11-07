use crate::structured::{ARRAYS, YAML, merge};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "yaml.merge",
    summary: "Deep-merge files; later files win",
    input: Input::all(YAML, 2, "yaml"),
    output: Output::one(&[Format::Yaml], "yaml"),
    params: &[ARRAYS],
    examples: &["deft yaml merge config.yaml override.yaml -o merged.yaml"],
    tags: &["combine yaml", "merge yaml", "override config", "overlay"],
    suffix: Some("merged"),
    run: merge,
    ..Op::DEFAULT
};
