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

pub const SHEET: ParamSpec = param("sheet", Kind::Text, "Sheet name or number (default: first)");
pub const TO: ParamSpec = param(
    "to",
    Kind::Choice(&["csv", "xlsx", "json"]),
    "Output format (default: like the input)",
);

pub fn tables(input: &InputFile) -> Result<Vec<Table>> {
    deft_tools_sheet::read(input.bytes()?, input.require_format()?, &input.stem())
}

/// A warning when a workbook has sheets that a command will not look at.
pub fn other_sheets(tables: &[Table], wanted: Option<&str>) -> Option<String> {
    let rest: Vec<&str> = tables.iter().skip(1).map(|t| t.name.as_str()).collect();
    (wanted.is_none() && !rest.is_empty()).then(|| {
        format!(
            "used the first sheet, {}; also in the file: {}",
            tables[0].name,
            rest.join(", ")
        )
    })
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

/// Pick a sheet by name or 1-based number.
pub fn pick(mut tables: Vec<Table>, wanted: Option<String>) -> Result<Table> {
    let index = match wanted {
        Some(w) => sheet_index(&tables, &w)?,
        None => 0,
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

/// The table a one-input operation works on, and a warning if the file has
/// more sheets than the one taken.
pub fn input_table(job: &Job) -> Result<(Table, Option<String>)> {
    let sheets = tables(job.input())?;
    let wanted = job.params.opt_text("sheet");
    let warning = other_sheets(&sheets, wanted.as_deref()).map(|w| format!("{w} (see --sheet)"));
    Ok((pick(sheets, wanted)?, warning))
}

/// Add a warning, if there is one.
pub fn warned(res: OpResult, warning: Option<String>) -> OpResult {
    match warning {
        Some(w) => res.warn(w),
        None => res,
    }
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
