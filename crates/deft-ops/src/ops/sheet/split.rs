use super::_common::{IN, OUT, SHEET, TO, column, input_table, tables, warned, write};
use deft_core::prelude::*;
use deft_tools_sheet::Table;

pub static OP: Op = Op {
    id: "sheet.split",
    summary: "Split up: --sheets, --columns city or --every 1000",
    details: &["--sheets writes each sheet to its own file; the others split rows."],
    input: Input::each(IN, "sheet"),
    output: Output::many(OUT, "sheet"),
    params: &[
        param("sheets", Kind::Flag, "One file per sheet of a workbook"),
        param("columns", Kind::Text, "One file per value of this column"),
        param(
            "every",
            Kind::Int {
                min: 1,
                max: 100_000_000,
            },
            "Rows per file",
        ),
        SHEET,
        TO,
    ],
    examples: &[
        "deft sheet split book.xlsx --sheets -o parts/",
        "deft sheet split people.csv --every 1 --to json -o rows/",
    ],
    tags: &[
        "all sheets",
        "each sheet",
        "chunk",
        "export every sheet",
        "partition",
    ],
    result: &["files"],
    suffix: Some("split"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let stem = job.input().stem();
    let mut warning = None;
    let parts: Vec<(String, Table)> = if p.flag("sheets") {
        tables(job.input())?
            .into_iter()
            .map(|t| (t.name.clone(), t))
            .collect()
    } else {
        let (t, other_sheets) = input_table(job)?;
        warning = other_sheets;
        match (p.opt_text("columns"), p.opt_int("every")) {
            (Some(name), None) => {
                let c = column(&t, "columns", &name)?;
                deft_tools_sheet::split_by(&t, c)
                    .into_iter()
                    .map(|(k, part)| (format!("{stem}-{k}"), part))
                    .collect()
            }
            (None, Some(n)) => deft_tools_sheet::split_rows(&t, n as usize)
                .into_iter()
                .enumerate()
                .map(|(i, part)| (format!("{stem}-{:03}", i + 1), part))
                .collect(),
            _ => {
                return Err(err(
                    ErrorCode::BadOption,
                    "give one of --sheets, --columns or --every",
                ));
            }
        }
    };
    let input = job.input().format.filter(|f| OUT.contains(f));
    let format = p
        .opt_text("to")
        .and_then(|t| Format::from_name(&t))
        .or(input)
        .unwrap_or(Format::Csv);
    let count = parts.len();
    let outputs = parts
        .into_iter()
        .map(|(name, t)| Ok(write(&t, format)?.named(format!("{name}.{}", format.ext()))))
        .collect::<Result<Vec<_>>>()?;
    let res = OpResult::many(outputs)
        .fact("files", count)
        .note(format!("{count} files"));
    Ok(warned(res, warning))
}
