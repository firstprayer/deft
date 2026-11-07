use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "sheet",
    summary: "Excel/CSV/ODS tables: SQL query, pivot, merge, split, convert",
    aliases: &["excel", "xlsx", "csv", "table", "spreadsheet"],
    synonyms: &[
        ("filter", "query"),
        ("where", "query"),
        ("sort", "query"),
        ("join", "query"),
        ("vlookup", "query"),
        ("group", "query"),
        ("select", "query"),
        ("dedupe", "query"),
        ("unique", "query"),
        ("range", "extract"),
        ("markdown", "to-md"),
        ("sql", "query"),
        ("sqlite", "query"),
        ("crosstab", "pivot"),
        ("describe", "stats"),
        ("summary", "stats"),
    ],
    order: 45,
};
