//! Table capabilities: read XLSX/XLS/ODS/CSV/JSON into a simple table and
//! write CSV, XLSX, JSON and Markdown.

use calamine::{Data, Reader, open_workbook_auto_from_rs};
use deft_core::error::{ErrorCode, Result, err};
use deft_core::format::Format;
use std::io::Cursor;

mod ops;
mod query;

pub use ops::*;
pub use query::{Filter, parse_filter};

#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    Empty,
    Text(String),
    Number(f64),
    /// A number whose spelling must survive a round trip, such as `1.50`.
    Decimal(f64, String),
    Bool(bool),
}

impl Cell {
    /// The numeric value, when the cell holds one.
    pub fn number(&self) -> Option<f64> {
        match self {
            Cell::Number(n) | Cell::Decimal(n, _) => Some(*n),
            _ => None,
        }
    }
}

impl std::fmt::Display for Cell {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Cell::Empty => Ok(()),
            Cell::Text(s) => f.write_str(s),
            Cell::Number(n) if n.fract() == 0.0 && n.abs() < 1e15 => write!(f, "{}", *n as i64),
            Cell::Number(n) => write!(f, "{n}"),
            Cell::Decimal(_, s) => f.write_str(s),
            Cell::Bool(b) => write!(f, "{b}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Table {
    pub name: String,
    pub rows: Vec<Vec<Cell>>,
}

impl Table {
    pub fn columns(&self) -> usize {
        self.rows.iter().map(Vec::len).max().unwrap_or(0)
    }
}

/// Excel serial date to ISO text.
fn serial_to_iso(serial: f64) -> String {
    let days = serial.floor() as i64 - 25569; // days from 1970-01-01
    // Civil-from-days (Howard Hinnant).
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    let secs = ((serial - serial.floor()) * 86400.0).round() as i64;
    if secs == 0 {
        format!("{y:04}-{m:02}-{d:02}")
    } else {
        format!(
            "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02}",
            secs / 3600,
            secs / 60 % 60,
            secs % 60
        )
    }
}

fn cell(d: &Data) -> Cell {
    match d {
        Data::Empty | Data::Error(_) => Cell::Empty,
        Data::Int(i) => Cell::Number(*i as f64),
        Data::Float(f) => Cell::Number(*f),
        Data::String(s) => Cell::Text(s.clone()),
        Data::Bool(b) => Cell::Bool(*b),
        Data::DateTime(t) => Cell::Text(serial_to_iso(t.as_f64())),
        Data::DateTimeIso(s) | Data::DurationIso(s) => Cell::Text(s.clone()),
    }
}

/// Read every sheet of a workbook.
pub fn read_workbook(bytes: &[u8]) -> Result<Vec<Table>> {
    let mut wb = open_workbook_auto_from_rs(Cursor::new(bytes.to_vec()))
        .map_err(|e| err(ErrorCode::InputCorrupt, format!("workbook: {e}")))?;
    let mut out = Vec::new();
    for name in wb.sheet_names() {
        let range = wb
            .worksheet_range(&name)
            .map_err(|e| err(ErrorCode::InputCorrupt, format!("sheet {name}: {e}")))?;
        let rows = range.rows().map(|r| r.iter().map(cell).collect()).collect();
        out.push(Table { name, rows });
    }
    Ok(out)
}

fn parse_scalar(s: &str) -> Cell {
    if s.is_empty() {
        return Cell::Empty;
    }
    // Keep identifiers such as 007 or +1234 as text.
    let leading_zero = s.len() > 1 && s.starts_with('0') && !s.starts_with("0.");
    let plain = !leading_zero && !s.starts_with('+');
    match s.parse::<f64>() {
        Ok(n) if plain && n.is_finite() => {
            let cell = Cell::Number(n);
            if cell.to_string() == s {
                cell
            } else {
                Cell::Decimal(n, s.to_string())
            }
        }
        _ => Cell::Text(s.to_string()),
    }
}

/// Read CSV; the delimiter is detected from the first line.
pub fn read_csv(bytes: &[u8], name: &str) -> Result<Table> {
    let head = bytes.split(|b| *b == b'\n').next().unwrap_or_default();
    let delimiter = [b',', b'\t', b';', b'|']
        .into_iter()
        .max_by_key(|d| head.iter().filter(|b| *b == d).count())
        .unwrap_or(b',');
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let mut r = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delimiter)
        .from_reader(bytes);
    let mut rows = Vec::new();
    for rec in r.records() {
        let rec = rec.map_err(|e| err(ErrorCode::InputCorrupt, format!("csv: {e}")))?;
        rows.push(rec.iter().map(parse_scalar).collect());
    }
    Ok(Table {
        name: name.into(),
        rows,
    })
}

/// Read a JSON array of objects (or of arrays).
pub fn read_json(bytes: &[u8], name: &str) -> Result<Table> {
    use serde_json::Value;
    let v: Value = serde_json::from_slice(bytes)
        .map_err(|e| err(ErrorCode::InputCorrupt, format!("json: {e}")))?;
    let Value::Array(items) = v else {
        return Err(err(
            ErrorCode::InputUnsupported,
            "expected a JSON array of rows",
        ));
    };
    let to_cell = |v: &Value| match v {
        Value::Null => Cell::Empty,
        Value::Bool(b) => Cell::Bool(*b),
        Value::Number(n) => Cell::Number(n.as_f64().unwrap_or(0.0)),
        Value::String(s) => Cell::Text(s.clone()),
        other => Cell::Text(other.to_string()),
    };
    let mut rows = Vec::new();
    if items.iter().all(Value::is_object) {
        let mut header: Vec<String> = Vec::new();
        for item in &items {
            for k in item.as_object().expect("checked").keys() {
                if !header.contains(k) {
                    header.push(k.clone());
                }
            }
        }
        rows.push(header.iter().map(|h| Cell::Text(h.clone())).collect());
        for item in &items {
            let o = item.as_object().expect("checked");
            rows.push(
                header
                    .iter()
                    .map(|h| o.get(h).map(to_cell).unwrap_or(Cell::Empty))
                    .collect(),
            );
        }
    } else {
        for item in &items {
            match item {
                Value::Array(a) => rows.push(a.iter().map(to_cell).collect()),
                other => rows.push(vec![to_cell(other)]),
            }
        }
    }
    Ok(Table {
        name: name.into(),
        rows,
    })
}

/// Read any supported format into tables (one per sheet).
pub fn read(bytes: &[u8], format: Format, name: &str) -> Result<Vec<Table>> {
    match format {
        Format::Csv => Ok(vec![read_csv(bytes, name)?]),
        Format::Json => Ok(vec![read_json(bytes, name)?]),
        Format::Xlsx | Format::Xls | Format::Ods => read_workbook(bytes),
        other => Err(err(
            ErrorCode::InputUnsupported,
            format!("{other} is not a table format"),
        )),
    }
}

pub fn write_csv(t: &Table) -> Result<Vec<u8>> {
    let mut w = csv::WriterBuilder::new()
        .flexible(true)
        .from_writer(Vec::new());
    for row in &t.rows {
        w.write_record(row.iter().map(|c| c.to_string()))
            .map_err(|e| err(ErrorCode::Internal, e.to_string()))?;
    }
    w.into_inner()
        .map_err(|e| err(ErrorCode::Internal, e.to_string()))
}

/// JSON array of objects keyed by the first row.
pub fn write_json(t: &Table) -> Result<Vec<u8>> {
    use serde_json::{Map, Value, json};
    let Some((head, body)) = t.rows.split_first() else {
        return Ok(b"[]\n".to_vec());
    };
    let keys: Vec<String> = head
        .iter()
        .enumerate()
        .map(|(i, c)| {
            Some(c.to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| format!("column{}", i + 1))
        })
        .collect();
    let rows: Vec<Value> = body
        .iter()
        .map(|r| {
            let mut m = Map::new();
            for (i, k) in keys.iter().enumerate() {
                let v = match r.get(i) {
                    None | Some(Cell::Empty) => Value::Null,
                    Some(Cell::Text(s)) => json!(s),
                    Some(Cell::Number(n) | Cell::Decimal(n, _))
                        if n.fract() == 0.0 && n.abs() < 9e15 =>
                    {
                        json!(*n as i64)
                    }
                    Some(Cell::Number(n) | Cell::Decimal(n, _)) => json!(n),
                    Some(Cell::Bool(b)) => json!(b),
                };
                m.insert(k.clone(), v);
            }
            Value::Object(m)
        })
        .collect();
    let mut s =
        serde_json::to_vec_pretty(&rows).map_err(|e| err(ErrorCode::Internal, e.to_string()))?;
    s.push(b'\n');
    Ok(s)
}

pub fn write_markdown(t: &Table) -> Vec<u8> {
    let cols = t.columns().max(1);
    let line = |r: &[Cell]| {
        let cells: Vec<String> = (0..cols)
            .map(|i| {
                r.get(i)
                    .map(|c| c.to_string().replace('|', "\\|").replace('\n', " "))
                    .unwrap_or_default()
            })
            .collect();
        format!("| {} |\n", cells.join(" | "))
    };
    let mut s = String::new();
    if let Some((head, body)) = t.rows.split_first() {
        s.push_str(&line(head));
        s.push_str(&format!("|{}\n", " --- |".repeat(cols)));
        for r in body {
            s.push_str(&line(r));
        }
    }
    s.into_bytes()
}

/// Write tables to an XLSX workbook, one sheet each. The creation date is
/// fixed so the output is reproducible.
pub fn write_xlsx(tables: &[Table]) -> Result<Vec<u8>> {
    use rust_xlsxwriter::{DocProperties, ExcelDateTime, Workbook};
    let x = |e: rust_xlsxwriter::XlsxError| err(ErrorCode::Internal, format!("xlsx: {e}"));
    let mut wb = Workbook::new();
    let date = ExcelDateTime::from_ymd(2000, 1, 1).map_err(x)?;
    wb.set_properties(&DocProperties::new().set_creation_datetime(&date));
    for t in tables {
        let ws = wb.add_worksheet();
        let name: String = t
            .name
            .chars()
            .filter(|c| !"[]:*?/\\".contains(*c))
            .take(31)
            .collect();
        if !name.is_empty() {
            ws.set_name(name).map_err(x)?;
        }
        for (r, row) in t.rows.iter().enumerate() {
            for (c, v) in row.iter().enumerate() {
                let (r, c) = (r as u32, c as u16);
                match v {
                    Cell::Empty => {}
                    Cell::Text(s) => {
                        ws.write_string(r, c, s).map_err(x)?;
                    }
                    Cell::Number(n) | Cell::Decimal(n, _) => {
                        ws.write_number(r, c, *n).map_err(x)?;
                    }
                    Cell::Bool(b) => {
                        ws.write_boolean(r, c, *b).map_err(x)?;
                    }
                }
            }
        }
    }
    wb.save_to_buffer().map_err(x)
}

/// Parse an A1-style range such as `B2:D10` into zero-based inclusive bounds.
pub fn parse_range(s: &str) -> Option<(usize, usize, usize, usize)> {
    fn cell(s: &str) -> Option<(usize, usize)> {
        let split = s.find(|c: char| c.is_ascii_digit())?;
        let (letters, digits) = s.split_at(split);
        if letters.is_empty() {
            return None;
        }
        let mut col = 0usize;
        for c in letters.chars() {
            if !c.is_ascii_alphabetic() {
                return None;
            }
            col = col * 26 + (c.to_ascii_uppercase() as usize - 'A' as usize + 1);
        }
        let row: usize = digits.parse().ok()?;
        (row > 0).then_some((row - 1, col - 1))
    }
    let (a, b) = s.split_once(':').unwrap_or((s, s));
    let (r1, c1) = cell(a.trim())?;
    let (r2, c2) = cell(b.trim())?;
    Some((r1.min(r2), c1.min(c2), r1.max(r2), c1.max(c2)))
}

pub fn slice(t: &Table, range: (usize, usize, usize, usize)) -> Table {
    let (r1, c1, r2, c2) = range;
    // A range past the data, like A1:ZZ11, means "up to the last column".
    let c2 = c2.min(t.columns().saturating_sub(1)).max(c1);
    let rows = t
        .rows
        .iter()
        .skip(r1)
        .take(r2 + 1 - r1)
        .map(|row| {
            (c1..=c2)
                .map(|c| row.get(c).cloned().unwrap_or(Cell::Empty))
                .collect()
        })
        .collect();
    Table {
        name: t.name.clone(),
        rows,
    }
}

/// Append tables; when all share the same first row, it is kept once.
pub fn concat(tables: &[Table]) -> Table {
    let same_header = tables
        .windows(2)
        .all(|w| w[0].rows.first() == w[1].rows.first());
    let mut rows = Vec::new();
    for (i, t) in tables.iter().enumerate() {
        let skip = usize::from(i > 0 && same_header);
        rows.extend(t.rows.iter().skip(skip).cloned());
    }
    Table {
        name: tables.first().map(|t| t.name.clone()).unwrap_or_default(),
        rows,
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
