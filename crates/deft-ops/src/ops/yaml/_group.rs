use crate::structured::SYNONYMS;
use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "yaml",
    summary: "YAML: convert, format, merge, diff, validate",
    aliases: &["yml"],
    synonyms: SYNONYMS,
    order: 46,
};
