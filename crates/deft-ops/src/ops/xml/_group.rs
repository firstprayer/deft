use crate::structured::SYNONYMS;
use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "xml",
    summary: "XML: convert, format, merge, diff, validate",
    aliases: &[],
    synonyms: SYNONYMS,
    order: 48,
};
