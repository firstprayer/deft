use crate::structured::SYNONYMS;
use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "xml",
    summary: "XML: convert to JSON, YAML or TOML",
    aliases: &[],
    synonyms: SYNONYMS,
    order: 47,
};
