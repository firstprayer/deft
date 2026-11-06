use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "data",
    summary: "JSON/YAML/TOML/XML: convert, query with jq syntax, merge, diff",
    aliases: &["json", "yaml", "toml", "xml"],
    synonyms: &[
        ("jq", "query"),
        ("get", "query"),
        ("filter", "query"),
        ("pretty", "format"),
        ("prettify", "format"),
        ("minify", "format"),
        ("lint", "validate"),
        ("check", "validate"),
        ("compare", "diff"),
    ],
    order: 45,
};
