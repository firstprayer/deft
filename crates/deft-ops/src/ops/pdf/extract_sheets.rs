use super::_common::{PASSWORD, open, plural, resolve, table_cell};
use deft_core::prelude::*;
use deft_tools_sheet::Table;

pub static OP: Op = Op {
    id: "pdf.extract-sheets",
    summary: "Save each table apart",
    details: &[
        "Text around tables is left out. A table that runs onto the next page stays whole.",
        "Tables are found by text position; check the result. Scans need pdf ocr first.",
    ],
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::many(&[Format::Csv, Format::Xlsx], "csv|xlsx"),
    params: &[
        param("pages", Kind::Pages, "Pages to read").default("all"),
        param(
            "to",
            Kind::Choice(&["csv", "xlsx"]),
            "Target format (or give -o name.xlsx)",
        )
        .default("csv"),
        PASSWORD,
    ],
    stability: Stability::Beta,
    examples: &[
        "deft pdf extract-sheets tables.pdf -o out/",
        "deft pdf extract-sheets tables.pdf --to xlsx -o all.xlsx",
    ],
    tags: &[
        "multiple tables",
        "all tables",
        "extract tables",
        "pdf to excel",
    ],
    result: &["tables"],
    suffix: Some("tables"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let doc = open(job.input(), job.params)?;
    let ids = deft_tools_pdf::page_ids(&doc);
    let indexes = resolve(&job.params.pages("pages"), ids.len())?;
    let mut pages = Vec::with_capacity(indexes.len());
    deft_tools_pdf::text::reader(&doc)?.each_page_text(&indexes, |_, page| {
        pages.push(deft_tools_pdf::layout::tables(&page));
    });
    let stem = job.input().stem();
    let tables: Vec<Table> = deft_tools_pdf::layout::join_pages(pages)
        .into_iter()
        .enumerate()
        .map(|(n, rows)| Table {
            name: format!("Table {}", n + 1),
            rows: rows
                .iter()
                .map(|row| row.iter().map(|c| table_cell(c)).collect())
                .collect(),
        })
        .collect();
    let count = tables.len();
    let note = plural(count, "table");
    if tables.is_empty() {
        return Ok(OpResult::many(vec![])
            .fact("tables", 0)
            .note(note)
            .warn("no tables found; a scan needs pdf ocr first"));
    }
    let xlsx = job.out_format == Some(Format::Xlsx) || job.params.text("to") == "xlsx";
    if xlsx {
        let bytes = deft_tools_sheet::write_xlsx(&tables)?;
        return Ok(OpResult::one(Produced::bytes(bytes, Format::Xlsx))
            .fact("tables", count)
            .note(note));
    }
    let mut outputs = Vec::new();
    for (n, table) in tables.iter().enumerate() {
        let bytes = deft_tools_sheet::write_csv(table)?;
        outputs.push(
            Produced::bytes(bytes, Format::Csv)
                .named(format!("{stem}-table-{}.csv", n + 1))
                .fact("rows", table.rows.len()),
        );
    }
    Ok(OpResult::many(outputs).fact("tables", count).note(note))
}
