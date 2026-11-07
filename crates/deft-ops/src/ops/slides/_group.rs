use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "slides",
    summary: "PowerPoint: extract text, Markdown outline",
    aliases: &["pptx", "ppt", "powerpoint", "deck"],
    synonyms: &[
        ("text", "to-text"),
        ("txt", "to-text"),
        ("markdown", "to-md"),
        ("notes", "to-text"),
    ],
    order: 42,
};
