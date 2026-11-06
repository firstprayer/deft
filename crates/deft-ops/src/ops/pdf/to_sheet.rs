use super::_common::{PASSWORD, open, resolve};
use deft_core::prelude::*;
use deft_tools_sheet::{Cell, Table};

const OUT: &[Format] = &[Format::Csv, Format::Xlsx, Format::Json];

pub static OP: Op = Op {
    id: "pdf.to-sheet",
    summary: "Pull tables out by text position",
    details: &[
        "Columns come from left edges that repeat down the page; check the result.",
        "Ruled and multi-line cells may split. Scans need pdf ocr first, and still may not work.",
    ],
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::one(OUT, "sheet"),
    params: &[
        param("pages", Kind::Pages, "Pages to read").default("all"),
        param(
            "to",
            Kind::Choice(&["csv", "xlsx", "json"]),
            "Target format (or give -o name.ext)",
        )
        .default("csv"),
        PASSWORD,
    ],
    mounts: &["sheet from-pdf"],
    converts: &[Edge {
        from: &[Format::Pdf],
        to: OUT,
        quality: Quality::Approximate,
    }],
    stability: Stability::Beta,
    examples: &["deft pdf to-sheet report.pdf --to xlsx -o tables.xlsx"],
    tags: &["pdf to excel", "extract table", "tabula", "statement"],
    result: &["rows", "columns"],
    run,
    ..Op::DEFAULT
};

fn cell(text: &str) -> Cell {
    if text.is_empty() {
        return Cell::Empty;
    }
    let clean = text.replace(',', "");
    match clean.parse::<f64>() {
        Ok(n) if !text.starts_with('0') || text.starts_with("0.") || text == "0" => {
            Cell::Decimal(n, text.to_string())
        }
        _ => Cell::Text(text.to_string()),
    }
}

fn run(job: &Job) -> Result<OpResult> {
    let doc = open(job.input(), job.params)?;
    let ids = deft_tools_pdf::page_ids(&doc);
    let indexes = resolve(&job.params.pages("pages"), ids.len())?;
    let mut rows = Vec::new();
    for &i in &indexes {
        for row in deft_tools_pdf::layout::table(&doc, ids[i]) {
            rows.push(row.iter().map(|c| cell(c)).collect());
        }
    }
    let table = Table {
        name: job.input().stem(),
        rows,
    };
    let format = Format::from_name(&job.params.text("to")).expect("choice");
    let bytes = match format {
        Format::Xlsx => deft_tools_sheet::write_xlsx(std::slice::from_ref(&table))?,
        Format::Json => deft_tools_sheet::write_json(&table)?,
        _ => deft_tools_sheet::write_csv(&table)?,
    };
    let mut res = OpResult::one(Produced::bytes(bytes, format))
        .fact("rows", table.rows.len())
        .fact("columns", table.columns())
        .note(format!(
            "{} rows x {} columns",
            table.rows.len(),
            table.columns()
        ));
    if table.rows.is_empty() {
        res = res.warn("no text found; the PDF may be a scan");
    }
    Ok(res)
}
