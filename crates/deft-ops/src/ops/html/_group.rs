use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "html",
    summary: "HTML to Markdown, text, Word, PDF; screenshots",
    aliases: &["htm", "webpage"],
    synonyms: &[
        ("markdown", "to-md"),
        ("text", "to-text"),
        ("txt", "to-text"),
        ("pdf", "to-pdf"),
        ("docx", "to-docx"),
        ("png", "screenshot"),
        ("image", "screenshot"),
    ],
    order: 52,
};
