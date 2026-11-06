use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "slides",
    summary: "PowerPoint: extract text, Markdown outline, PDF handout",
    aliases: &["pptx", "ppt", "powerpoint", "deck"],
    synonyms: &[
        ("pdf", "to-pdf"),
        ("text", "to-text"),
        ("txt", "to-text"),
        ("markdown", "to-md"),
        ("notes", "to-text"),
    ],
    order: 42,
};
