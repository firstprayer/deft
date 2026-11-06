use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "email",
    summary: "EML files: headers, text, attachments",
    aliases: &["eml", "mail"],
    synonyms: &[
        ("text", "to-text"),
        ("txt", "to-text"),
        ("extract", "attachments"),
    ],
    order: 56,
};
