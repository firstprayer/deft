use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "archive",
    summary: "zip/tar/gz/xz/zst/7z: create, extract, list; ZIP passwords",
    aliases: &["zip", "tar", "compress-files"],
    synonyms: &[("unpack", "extract"), ("pack", "create"), ("zip", "create")],
    order: 60,
};
