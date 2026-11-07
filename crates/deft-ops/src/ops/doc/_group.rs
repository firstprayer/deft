use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "doc",
    summary: "Word: to text, Markdown, HTML, PDF; find and replace",
    aliases: &["docx", "word"],
    synonyms: &[
        ("pdf", "to-pdf"),
        ("text", "to-text"),
        ("txt", "to-text"),
        ("markdown", "to-md"),
        ("html", "to-html"),
        ("sub", "replace"),
        ("sed", "replace"),
    ],
    order: 40,
};
