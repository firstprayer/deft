use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "sheet",
    summary: "Excel/CSV/ODS tables: pivot, merge, split, convert",
    aliases: &["excel", "xlsx", "csv", "table", "spreadsheet"],
    synonyms: &[
        ("range", "extract"),
        ("markdown", "to-md"),
        ("crosstab", "pivot"),
        ("describe", "stats"),
        ("summary", "stats"),
    ],
    order: 45,
};
