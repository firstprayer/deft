use crate::structured::{JSON, SORT_KEYS, format};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "json.format",
    summary: "Pretty-print or minify: --compact, --sort-keys",
    input: Input::each(JSON, "json"),
    output: Output::one(&[Format::Json], "json"),
    params: &[
        SORT_KEYS,
        param("compact", Kind::Flag, "All on one line"),
        param("indent", Kind::Int { min: 1, max: 16 }, "Spaces per level").default("2"),
    ],
    examples: &[
        "deft json format messy.json -o tidy.json",
        "deft json format messy.json --compact --sort-keys -o -",
    ],
    tags: &[
        "pretty print json",
        "minify json",
        "beautify json",
        "indent",
        "sort keys",
    ],
    suffix: Some("formatted"),
    run: format,
    ..Op::DEFAULT
};
