use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "html",
    summary: "HTML to Markdown, text, Word and PDF",
    aliases: &["htm", "webpage"],
    synonyms: &[
        ("markdown", "to-md"),
        ("text", "to-text"),
        ("txt", "to-text"),
        ("pdf", "to-pdf"),
        ("docx", "to-docx"),
    ],
    order: 52,
};
