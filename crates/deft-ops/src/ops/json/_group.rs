use crate::structured::SYNONYMS;
use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "json",
    summary: "JSON: convert, validate against a schema",
    aliases: &["jsonl", "ndjson"],
    synonyms: SYNONYMS,
    order: 47,
};
