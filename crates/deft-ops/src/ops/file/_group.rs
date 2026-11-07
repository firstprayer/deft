use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "file",
    summary: "Any file: find duplicates, fix text encoding",
    aliases: &["files"],
    synonyms: &[
        ("dedupe", "duplicates"),
        ("dupes", "duplicates"),
        ("charset", "encoding"),
        ("iconv", "encoding"),
    ],
    order: 75,
};
