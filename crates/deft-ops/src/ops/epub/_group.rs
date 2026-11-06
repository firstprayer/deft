use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "epub",
    summary: "E-books: info, to text, Markdown and PDF",
    aliases: &["ebook", "book"],
    synonyms: &[
        ("markdown", "to-md"),
        ("text", "to-text"),
        ("txt", "to-text"),
        ("pdf", "to-pdf"),
    ],
    order: 54,
};
