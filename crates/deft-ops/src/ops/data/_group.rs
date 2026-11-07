use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "data",
    summary: "JSON/YAML/TOML/XML: convert, format, merge, diff, validate",
    aliases: &["json", "yaml", "toml", "xml"],
    synonyms: &[
        ("pretty", "format"),
        ("prettify", "format"),
        ("minify", "format"),
        ("lint", "validate"),
        ("check", "validate"),
        ("compare", "diff"),
    ],
    order: 45,
};
