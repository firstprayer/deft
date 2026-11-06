use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "md",
    summary: "Markdown to PDF, HTML, Word, EPUB; table of contents",
    aliases: &["markdown"],
    synonyms: &[
        ("pdf", "to-pdf"),
        ("html", "to-html"),
        ("docx", "to-docx"),
        ("word", "to-docx"),
        ("epub", "to-epub"),
        ("contents", "toc"),
    ],
    order: 50,
};
