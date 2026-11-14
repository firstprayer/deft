//! Helpers shared by the sheet operations.

use deft_core::prelude::*;
use deft_tools_sheet::Table;

pub const IN: &[Format] = &[
    Format::Xlsx,
    Format::Xls,
    Format::Ods,
    Format::Csv,
    Format::Json,
];
pub const OUT: &[Format] = &[Format::Csv, Format::Xlsx, Format::Json];

pub const SHEET: ParamSpec = param(
    "sheet",
    Kind::Text,
    "Sheet name or number (default: first visible)",
);
pub const TO: ParamSpec = param(
    "to",
    Kind::Choice(&["csv", "xlsx", "json"]),
    "Output format (default: like the input)",
);

pub fn tables(input: &InputFile) -> Result<Vec<Table>> {
    deft_tools_sheet::read(input.bytes()?, input.require_format()?, &input.stem())
}

/// Names for a message: the first few, then how many more.
pub fn some_names(names: &[&str]) -> String {
    const SHOWN: usize = 5;
    let mut s = names
        .iter()
        .take(SHOWN)
        .copied()
        .collect::<Vec<_>>()
        .join(", ");
    if names.len() > SHOWN {
        s.push_str(&format!(" and {} more", names.len() - SHOWN));
    }
    s
}

/// The sheet taken when none is named: the first one that is not hidden.
pub fn first_visible(tables: &[Table]) -> usize {
    tables.iter().position(|t| !t.hidden).unwrap_or(0)
}

/// A warning when a workbook has sheets that a command will not look at.
pub fn other_sheets(tables: &[Table], wanted: Option<&str>) -> Option<String> {
    if wanted.is_some() || tables.len() < 2 {
        return None;
    }
    let used = first_visible(tables);
    let rest: Vec<&str> = tables
        .iter()
        .enumerate()
        .filter(|(i, t)| *i != used && !t.hidden)
        .map(|(_, t)| t.name.as_str())
        .collect();
    let hidden: Vec<&str> = tables
        .iter()
        .enumerate()
        .filter(|(i, t)| *i != used && t.hidden)
        .map(|(_, t)| t.name.as_str())
        .collect();
    let mut w = format!("used the first sheet, {}", tables[used].name);
    if tables[used].hidden {
        w.push_str(" (hidden; every sheet is)");
    }
    if !rest.is_empty() {
        w.push_str(&format!("; also in the file: {}", some_names(&rest)));
    }
    if !hidden.is_empty() {
        w.push_str(&format!("; hidden sheets: {}", some_names(&hidden)));
    }
    Some(w)
}

/// Find a sheet by name or 1-based number.
pub fn sheet_index(tables: &[Table], wanted: &str) -> Result<usize> {
    let names: Vec<&str> = tables.iter().map(|t| t.name.as_str()).collect();
    let index = match wanted.parse::<usize>() {
        Ok(n) if n >= 1 && n <= tables.len() => Some(n - 1),
        _ => names.iter().position(|n| n.eq_ignore_ascii_case(wanted)),
    };
    index.ok_or_else(|| {
        bad_value(
            "sheet",
            format!("no sheet {wanted:?}; sheets: {}", names.join(", ")),
        )
    })
}

/// Pick a sheet by name or 1-based number; by default the first that is
/// not hidden.
pub fn pick(mut tables: Vec<Table>, wanted: Option<String>) -> Result<Table> {
    let index = match wanted {
        Some(w) => sheet_index(&tables, &w)?,
        None => first_visible(&tables),
    };
    Ok(tables.swap_remove(index))
}

pub fn write(t: &Table, format: Format) -> Result<Produced> {
    let bytes = match format {
        Format::Csv => deft_tools_sheet::write_csv(t)?,
        Format::Json => deft_tools_sheet::write_json(t)?,
        Format::Xlsx => deft_tools_sheet::write_xlsx(std::slice::from_ref(t))?,
        Format::Md => deft_tools_sheet::write_markdown(t),
        other => {
            return Err(err(
                ErrorCode::InputUnsupported,
                format!("cannot write tables as {other}"),
            ));
        }
    };
    Ok(Produced::bytes(bytes, format))
}

pub fn shape(t: &Table) -> String {
    format!("{} rows x {} columns", t.rows.len(), t.columns())
}

/// The table a one-input operation works on, and warnings: other sheets
/// in the file, the text encoding a CSV file was read in.
pub fn input_table(job: &Job) -> Result<(Table, Vec<String>)> {
    let sheets = tables(job.input())?;
    let wanted = job.params.opt_text("sheet");
    let mut warnings: Vec<String> = other_sheets(&sheets, wanted.as_deref())
        .map(|w| format!("{w} (see --sheet)"))
        .into_iter()
        .collect();
    // Rows from a list inside a JSON object: the rest of the file is not in them.
    let nested = job.input().format == Some(Format::Json) && sheets[0].name != job.input().stem();
    if warnings.is_empty() && wanted.is_none() && nested {
        warnings.push(format!(
            "rows from {}, the list of records in the file; the rest is left out",
            sheets[0].name
        ));
    }
    let t = pick(sheets, wanted)?;
    warnings.extend(t.notes.iter().cloned());
    Ok((t, warnings))
}

/// Add the warnings.
pub fn warned(mut res: OpResult, warnings: Vec<String>) -> OpResult {
    for w in warnings {
        res = res.warn(w);
    }
    res
}

/// When named columns are not in the first row but all are in a later one
/// (a bank statement or a statistics table with a title block above its
/// header), take that row as the header. Returns the warning to give.
pub fn find_header(t: &mut Table, names: &[String]) -> Option<String> {
    let in_row = |row: &[deft_tools_sheet::Cell]| {
        names.iter().all(|n| {
            row.iter()
                .any(|c| c.to_string().trim().eq_ignore_ascii_case(n.trim()))
        })
    };
    if names.is_empty() || t.rows.first().is_none_or(|r| in_row(r)) {
        return None;
    }
    if deft_tools_sheet::find_columns(t, &names.join(",")).is_ok() {
        return None;
    }
    let at = t.rows.iter().take(50).position(|r| in_row(r))?;
    t.rows.drain(..at);
    Some(format!(
        "the header is row {}; the {} above it {} left out",
        at + 1,
        if at == 1 {
            "row".to_string()
        } else {
            format!("{at} rows")
        },
        if at == 1 { "was" } else { "were" }
    ))
}

/// Map a column lookup failure to the option that named the column.
pub fn columns(t: &Table, option: &str, list: &str) -> Result<Vec<usize>> {
    deft_tools_sheet::find_columns(t, list).map_err(|m| bad_value(option, m))
}

pub fn column(t: &Table, option: &str, name: &str) -> Result<usize> {
    deft_tools_sheet::find_column(t, name).map_err(|m| bad_value(option, m))
}

/// The output format: `--to`, then `-o name.ext`, then the input's format.
pub fn target(job: &Job) -> Format {
    let input = job.input().format.filter(|f| OUT.contains(f));
    job.params
        .opt_text("to")
        .and_then(|t| Format::from_name(&t))
        .unwrap_or_else(|| job.target(input.unwrap_or(Format::Csv)))
}

/// Write the result of a transform.
pub fn finish(job: &Job, t: &Table) -> Result<OpResult> {
    Ok(OpResult::one(write(t, target(job))?)
        .fact("rows", t.rows.len().saturating_sub(1))
        .fact("columns", t.columns())
        .note(format!(
            "{} rows x {} columns",
            t.rows.len().saturating_sub(1),
            t.columns()
        )))
}

#[cfg(test)]
mod tests {
    use super::*;
    use deft_tools_sheet::Cell;

    fn table(rows: &[&[&str]]) -> Table {
        Table {
            rows: rows
                .iter()
                .map(|r| r.iter().map(|c| Cell::Text(c.to_string())).collect())
                .collect(),
            ..Table::default()
        }
    }

    #[test]
    fn a_title_block_above_the_header_is_found_by_the_named_columns() {
        let mut t = table(&[
            &["Statement"],
            &["date", "name", "amount"],
            &["1", "a", "2"],
        ]);
        let w = find_header(&mut t, &["name".into(), "amount".into()]).unwrap();
        assert_eq!(w, "the header is row 2; the row above it was left out");
        assert_eq!(t.rows.len(), 2);
        // A header already in row 1 is left alone, as is a name found nowhere.
        assert!(find_header(&mut t, &["name".into()]).is_none());
        assert!(find_header(&mut t, &["nope".into()]).is_none());
    }

    #[test]
    fn hidden_sheets_are_skipped_by_default() {
        let mut hidden = table(&[&["x"]]);
        hidden.name = "Cache".into();
        hidden.hidden = true;
        let mut shown = table(&[&["y"]]);
        shown.name = "Data".into();
        let tables = vec![hidden, shown];
        assert_eq!(first_visible(&tables), 1);
        let w = other_sheets(&tables, None).unwrap();
        assert_eq!(w, "used the first sheet, Data; hidden sheets: Cache");
    }
}
