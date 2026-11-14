use super::_common::{
    IN, OUT, SHEET, TO, column, find_header, input_table, some_names, tables, warned, write,
};
use deft_core::prelude::*;
use deft_tools_sheet::Table;

pub static OP: Op = Op {
    id: "sheet.split",
    summary: "Split up: --sheets, --columns city or --every 1000",
    details: &[
        "--sheets writes each sheet to its own file, hidden ones left out; the others split rows.",
    ],
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
    let mut warning = Vec::new();
    let parts: Vec<(String, Table)> = if p.flag("sheets") {
        let all = tables(job.input())?;
        let hidden: Vec<&str> = all
            .iter()
            .filter(|t| t.hidden)
            .map(|t| t.name.as_str())
            .collect();
        // Hidden sheets are left out, unless every sheet is hidden.
        let skip = !hidden.is_empty() && hidden.len() < all.len();
        if skip {
            warning.push(format!(
                "left out {} hidden sheet{}: {} (use --sheet NAME with deft sheet convert for one)",
                hidden.len(),
                if hidden.len() == 1 { "" } else { "s" },
                some_names(&hidden)
            ));
        }
        all.into_iter()
            .filter(|t| !(skip && t.hidden))
            .map(|t| {
                warning.extend(t.notes.iter().cloned());
                (t.name.clone(), t)
            })
            .collect()
    } else {
        let (mut t, other_sheets) = input_table(job)?;
        warning = other_sheets;
        match (p.opt_text("columns"), p.opt_int("every")) {
            (Some(name), None) => {
                warning.extend(find_header(&mut t, std::slice::from_ref(&name)));
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
