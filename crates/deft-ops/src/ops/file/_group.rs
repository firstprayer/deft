use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "file",
    summary: "Any file: hash, duplicates, batch rename, base64, encoding",
    aliases: &["files"],
    synonyms: &[
        ("checksum", "hash"),
        ("sha256", "hash"),
        ("md5", "hash"),
        ("dedupe", "duplicates"),
        ("dupes", "duplicates"),
        ("mv", "rename"),
        ("charset", "encoding"),
        ("iconv", "encoding"),
        ("sort", "organize"),
    ],
    order: 75,
};
