use crate::structured::SYNONYMS;
use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "json",
    summary: "JSON: convert, format, merge, diff, validate",
    aliases: &["jsonl", "ndjson"],
    synonyms: SYNONYMS,
    order: 45,
};
