//! Table capabilities: read XLSX/XLS/ODS/CSV/JSON into a simple table and
//! write CSV, XLSX, JSON and Markdown.

use calamine::{Data, Reader, open_workbook_auto_from_rs};
use deft_core::error::{ErrorCode, Result, err};
use deft_core::format::Format;
use regex::Regex;
use std::io::Cursor;
use std::sync::LazyLock;

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
    /// Row and column of the first cell, counted from 0. A sheet need not
    /// start at A1, and readers drop the empty rows and columns before it.
    pub origin: (usize, usize),
    /// A sheet hidden in the workbook (Excel's hidden and very hidden, an
    /// ODS table with display off).
    pub hidden: bool,
    /// What reading had to decide, such as the text encoding of a CSV file.
    pub notes: Vec<String>,
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

/// ODS cells can hold pictures; a picture's title and description are not
/// cell text, but calamine collects all text inside a cell. Drop drawings
/// from content.xml before reading.
fn without_drawings(bytes: &[u8]) -> Option<Vec<u8>> {
    use std::io::{Read, Write};
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).ok()?;
    let mut content = String::new();
    zip.by_name("content.xml")
        .ok()?
        .read_to_string(&mut content)
        .ok()?;
    if !content.contains("<draw:") {
        return None;
    }
    let cleaned = strip_elements(&content, "draw:");
    let mut out = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).ok()?;
        let name = f.name().to_string();
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Stored);
        out.start_file(name.as_str(), options).ok()?;
        if name == "content.xml" {
            out.write_all(cleaned.as_bytes()).ok()?;
        } else {
            let mut buf = Vec::new();
            f.read_to_end(&mut buf).ok()?;
            out.write_all(&buf).ok()?;
        }
    }
    Some(out.finish().ok()?.into_inner())
}

/// Remove every element whose name starts with `prefix` (such as `draw:`),
/// with what it contains.
fn strip_elements(xml: &str, prefix: &str) -> String {
    let open = format!("<{prefix}");
    let mut out = String::with_capacity(xml.len());
    let mut rest = xml;
    while let Some(at) = rest.find(&open) {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        let Some(end) = tail.find('>') else {
            return out;
        };
        if tail[..end].ends_with('/') {
            rest = &tail[end + 1..];
            continue;
        }
        // Skip to the matching close tag, counting nested drawings.
        let mut depth = 0usize;
        let mut i = 0;
        let close = format!("</{prefix}");
        loop {
            let next_open = tail[i..].find(&open).map(|n| n + i);
            let next_close = tail[i..].find(&close).map(|n| n + i);
            match (next_open, next_close) {
                (Some(o), Some(c)) if o < c => {
                    let e = tail[o..].find('>').map_or(tail.len(), |n| o + n);
                    if !tail[o..e].ends_with('/') {
                        depth += 1;
                    }
                    i = e + 1;
                }
                (_, Some(c)) => {
                    depth -= 1;
                    i = tail[c..].find('>').map_or(tail.len(), |n| c + n + 1);
                    if depth == 0 {
                        break;
                    }
                }
                _ => return out,
            }
        }
        rest = &tail[i..];
    }
    out.push_str(rest);
    out
}

/// Read every sheet of a workbook.
pub fn read_workbook(bytes: &[u8]) -> Result<Vec<Table>> {
    let cleaned = if bytes.starts_with(b"PK") {
        without_drawings(bytes)
    } else {
        None
    };
    let bytes = cleaned.as_deref().unwrap_or(bytes);
    let mut wb = open_workbook_auto_from_rs(Cursor::new(bytes.to_vec()))
        .map_err(|e| err(ErrorCode::InputCorrupt, format!("workbook: {e}")))?;
    let hidden: std::collections::HashSet<String> = wb
        .sheets_metadata()
        .iter()
        .filter(|s| s.visible != calamine::SheetVisible::Visible)
        .map(|s| s.name.clone())
        .collect();
    let mut out = Vec::new();
    for name in wb.sheet_names() {
        let range = wb
            .worksheet_range(&name)
            .map_err(|e| err(ErrorCode::InputCorrupt, format!("sheet {name}: {e}")))?;
        let rows = range.rows().map(|r| r.iter().map(cell).collect()).collect();
        let origin = range
            .start()
            .map_or((0, 0), |(r, c)| (r as usize, c as usize));
        out.push(Table {
            hidden: hidden.contains(&name),
            name,
            rows,
            origin,
            notes: Vec::new(),
        });
    }
    Ok(out)
}

/// A number written the European way, such as 3,50 or 1.234,56.
fn comma_decimal(s: &str) -> Option<f64> {
    static FORM: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^-?(0|[1-9][0-9]*|[1-9][0-9]{0,2}(\.[0-9]{3})+),[0-9]+$").expect("valid")
    });
    FORM.is_match(s)
        .then(|| s.replace('.', "").replace(',', ".").parse().ok())
        .flatten()
}

/// Text that only looks like a number: an identifier such as 12E3, or a whole
/// number with more digits than a floating-point number can hold.
fn number_in_name_only(s: &str, n: f64) -> bool {
    let digits = s.strip_prefix('-').unwrap_or(s);
    let whole = digits.bytes().all(|b| b.is_ascii_digit());
    let bare_exponent = digits
        .split_once(['e', 'E'])
        .is_some_and(|(m, e)| !m.contains('.') && e.bytes().all(|b| b.is_ascii_digit()));
    // A whole number of 16 digits or more is an id (an invoice, a card, a
    // tracking number): Excel shows it as 2.02425E+15 and edits it with 15
    // digits. A round amount, ending in many zeros, stays a number.
    let zeros = digits.len() - digits.trim_end_matches('0').len();
    let long = whole && digits.len() >= 16 && zeros < 6;
    bare_exponent || long || (whole && format!("{n:.0}") != s)
}

pub(crate) fn parse_scalar(s: &str, decimal_comma: bool) -> Cell {
    if s.is_empty() {
        return Cell::Empty;
    }
    if decimal_comma && let Some(n) = comma_decimal(s) {
        return Cell::Decimal(n, s.to_string());
    }
    // Keep identifiers such as 007 or +1234 as text.
    let digits = s.strip_prefix('-').unwrap_or(s);
    let leading_zero = digits.len() > 1 && digits.starts_with('0') && !digits.starts_with("0.");
    let plain = !leading_zero && !s.starts_with('+');
    match s.parse::<f64>() {
        Ok(n) if plain && n.is_finite() && !number_in_name_only(s, n) => {
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

/// The delimiter of a CSV file: whichever is most common in its first line,
/// not counting quoted text. Ties go to the comma.
fn detect_delimiter(bytes: &[u8]) -> u8 {
    const CANDIDATES: [u8; 4] = [b',', b'\t', b';', b'|'];
    let mut counts = [0usize; 4];
    let mut quoted = false;
    for b in bytes {
        match b {
            b'"' => quoted = !quoted,
            b'\n' if !quoted => break,
            b if !quoted => {
                if let Some(i) = CANDIDATES.iter().position(|d| d == b) {
                    counts[i] += 1;
                }
            }
            _ => {}
        }
    }
    let best = counts.iter().copied().max().unwrap_or(0);
    let at = counts.iter().position(|c| *c == best).unwrap_or(0);
    CANDIDATES[at]
}

/// UTF-16 text with a byte order mark, which is what Excel calls Unicode Text.
fn decode_utf16(bytes: &[u8]) -> Option<String> {
    let (body, read): (_, fn([u8; 2]) -> u16) = match bytes {
        [0xFF, 0xFE, rest @ ..] => (rest, u16::from_le_bytes),
        [0xFE, 0xFF, rest @ ..] => (rest, u16::from_be_bytes),
        _ => return None,
    };
    let units = body.chunks_exact(2).map(|c| read([c[0], c[1]]));
    Some(
        char::decode_utf16(units)
            .map(|c| c.unwrap_or('\u{FFFD}'))
            .collect(),
    )
}

/// Text in a legacy encoding (Windows-1252, Latin-1, GBK, Shift-JIS...),
/// decoded, with the encoding's name. `None` for UTF-8.
fn decode_legacy(bytes: &[u8]) -> Option<(String, &'static str)> {
    if std::str::from_utf8(bytes).is_ok() {
        return None;
    }
    // UTF-8 text with a stray byte or two stays UTF-8.
    let (mut wide, mut invalid) = (0usize, 0usize);
    for chunk in bytes.utf8_chunks() {
        wide += chunk.valid().chars().filter(|c| !c.is_ascii()).count();
        invalid += usize::from(!chunk.invalid().is_empty());
    }
    if wide > 10 * invalid {
        return Some((
            String::from_utf8_lossy(bytes).into_owned(),
            "UTF-8 with some bad bytes",
        ));
    }
    let mut d = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Deny);
    d.feed(bytes, true);
    let enc = d.guess(None, chardetng::Utf8Detection::Deny);
    let (text, _, _) = enc.decode(bytes);
    // With few accented letters the guess may be a rarer single-byte code
    // page that reads them the same; name the common one then.
    let common = encoding_rs::WINDOWS_1252;
    if enc != common && enc.is_single_byte() && common.decode(bytes).0 == text {
        return Some((text.into_owned(), common.name()));
    }
    Some((text.into_owned(), enc.name()))
}

/// A number with a decimal point, such as 1148.7 or -0.25, that cannot be
/// a thousands group (1.234).
fn dot_decimal(s: &str) -> bool {
    let s = s.strip_prefix('-').unwrap_or(s);
    s.split_once('.').is_some_and(|(a, b)| {
        !a.is_empty()
            && !b.is_empty()
            && b.len() != 3
            && a.bytes().chain(b.bytes()).all(|c| c.is_ascii_digit())
    })
}

/// Read CSV; the delimiter is detected from the first line.
pub fn read_csv(bytes: &[u8], name: &str) -> Result<Table> {
    let mut notes = Vec::new();
    let mut decoded = decode_utf16(bytes);
    if decoded.is_none()
        && let Some((text, enc)) = decode_legacy(bytes)
    {
        notes.push(format!("{name}: the text is not UTF-8; read it as {enc}"));
        decoded = Some(text);
    }
    let bytes = decoded.as_deref().map_or(bytes, str::as_bytes);
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let delimiter = detect_delimiter(bytes);
    let mut records = Vec::new();
    let mut r = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .delimiter(delimiter)
        .from_reader(bytes);
    let mut rows: Vec<Vec<Cell>> = Vec::new();
    for rec in r.records() {
        let rec = rec.map_err(|e| match e.kind() {
            csv::ErrorKind::Utf8 { .. } => err(ErrorCode::InputCorrupt, "csv: not UTF-8 text")
                .hint("convert it to UTF-8 first, for example with iconv"),
            _ => err(ErrorCode::InputCorrupt, format!("csv: {e}")),
        })?;
        records.push(rec);
    }
    // A semicolon usually means comma decimals, but not in a file that writes
    // its numbers with a dot (1148.7), such as Brazil's open data.
    let decimal_comma = delimiter == b';' && !records.iter().flatten().any(dot_decimal);
    for rec in &records {
        rows.push(rec.iter().map(|s| parse_scalar(s, decimal_comma)).collect());
    }
    Ok(Table {
        name: name.into(),
        rows,
        notes,
        ..Table::default()
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
    Ok(json_rows(items, name))
}

/// The tables in a JSON file: the file itself when it is an array of rows;
/// otherwise every list of records inside it (an API reply's `"prizes"`, the
/// votes of an XML roll call), largest first, named by where it is, such as
/// `rollcall-vote.vote-data.recorded-vote`. Inside such a list, nested objects
/// become columns of their own (`legislator.@party`).
pub fn read_json_tables(bytes: &[u8], name: &str) -> Result<Vec<Table>> {
    use serde_json::Value;
    let v: Value = serde_json::from_slice(bytes)
        .map_err(|e| err(ErrorCode::InputCorrupt, format!("json: {e}")))?;
    if let Value::Array(items) = v {
        return Ok(vec![json_rows(items, name)]);
    }
    let mut found: Vec<(String, Vec<Value>)> = Vec::new();
    fn walk(v: &Value, path: &str, found: &mut Vec<(String, Vec<Value>)>) {
        match v {
            Value::Object(o) => {
                for (k, child) in o {
                    let p = if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}.{k}")
                    };
                    walk(child, &p, found);
                }
            }
            Value::Array(items) if !items.is_empty() && items.iter().all(Value::is_object) => {
                let rows = items.iter().map(flatten).collect();
                found.push((path.to_string(), rows));
            }
            _ => {}
        }
    }
    walk(&v, "", &mut found);
    if found.is_empty() {
        return Err(err(
            ErrorCode::InputUnsupported,
            "this JSON holds no list of records to make rows from",
        )
        .hint("deft json convert turns any JSON into YAML or TOML"));
    }
    found.sort_by_key(|(_, rows)| std::cmp::Reverse(rows.len()));
    Ok(found
        .into_iter()
        .map(|(path, rows)| json_rows(rows, &path))
        .collect())
}

/// An object with nested objects spread into dotted keys.
fn flatten(v: &serde_json::Value) -> serde_json::Value {
    use serde_json::{Map, Value};
    fn into(prefix: &str, v: &Value, out: &mut Map<String, Value>) {
        match v {
            Value::Object(o) if !o.is_empty() => {
                for (k, child) in o {
                    let key = if prefix.is_empty() {
                        k.clone()
                    } else {
                        format!("{prefix}.{k}")
                    };
                    into(&key, child, out);
                }
            }
            other => {
                out.insert(prefix.to_string(), other.clone());
            }
        }
    }
    let mut out = Map::new();
    into("", v, &mut out);
    Value::Object(out)
}

fn json_rows(items: Vec<serde_json::Value>, name: &str) -> Table {
    use serde_json::Value;
    let to_cell = |v: &Value| match v {
        Value::Null => Cell::Empty,
        Value::Bool(b) => Cell::Bool(*b),
        Value::Number(n) if n.is_f64() => Cell::Number(n.as_f64().unwrap_or(0.0)),
        Value::Number(n) => parse_scalar(&n.to_string(), false),
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
    Table {
        name: name.into(),
        rows,
        ..Table::default()
    }
}

/// Read any supported format into tables (one per sheet).
pub fn read(bytes: &[u8], format: Format, name: &str) -> Result<Vec<Table>> {
    match format {
        Format::Csv => Ok(vec![read_csv(bytes, name)?]),
        Format::Json => read_json_tables(bytes, name),
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

/// One name per column: an empty header becomes column3, and a repeated one
/// gets _2, ignoring case. A key may only appear once in JSON or in SQL.
pub(crate) fn unique_names(head: &[String], columns: usize) -> Vec<String> {
    let mut used = std::collections::HashSet::new();
    (0..columns)
        .map(|i| {
            let base = match head.get(i) {
                Some(h) if !h.is_empty() => h.clone(),
                _ => format!("column{}", i + 1),
            };
            let (mut name, mut n) = (base.clone(), 1);
            while !used.insert(name.to_lowercase()) {
                n += 1;
                name = format!("{base}_{n}");
            }
            name
        })
        .collect()
}

/// JSON array of objects keyed by the first row.
pub fn write_json(t: &Table) -> Result<Vec<u8>> {
    use serde_json::{Map, Value, json};
    let Some((head, body)) = t.rows.split_first() else {
        return Ok(b"[]\n".to_vec());
    };
    let names: Vec<String> = head.iter().map(|c| c.to_string()).collect();
    let keys = unique_names(&names, names.len());
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

/// The cells of a range, which counts from A1 wherever the data starts.
pub fn slice(t: &Table, range: (usize, usize, usize, usize)) -> Table {
    let (r1, c1, r2, c2) = range;
    let (top, left) = t.origin;
    // A range past the data, like A1:ZZ11, means "up to the last cell".
    let c2 = c2.min((left + t.columns()).saturating_sub(1)).max(c1);
    let r2 = r2.min((top + t.rows.len()).saturating_sub(1));
    let cell = |r: usize, c: usize| {
        let row = r.checked_sub(top).and_then(|r| t.rows.get(r));
        let found = row.and_then(|row| c.checked_sub(left).and_then(|c| row.get(c)));
        found.cloned().unwrap_or(Cell::Empty)
    };
    let rows = (r1..=r2)
        .map(|r| (c1..=c2).map(|c| cell(r, c)).collect())
        .collect();
    Table {
        name: t.name.clone(),
        rows,
        ..Table::default()
    }
}

/// Stack tables. Columns line up by header name and new ones go on the right,
/// so files may order their columns differently. Tables that share no header
/// with the first are stacked as they are; the second value says so.
pub fn concat(tables: &[Table]) -> (Table, bool) {
    let name = tables.first().map(|t| t.name.clone()).unwrap_or_default();
    let names = |t: &Table| -> Vec<String> { crate::headers(t) };
    let mut head = tables.first().map(&names).unwrap_or_default();
    let shares = |t: &Table| names(t).iter().any(|h| !h.is_empty() && head.contains(h));
    if !tables.iter().skip(1).all(shares) {
        let rows = tables.iter().flat_map(|t| t.rows.iter().cloned()).collect();
        let table = Table {
            name,
            rows,
            ..Table::default()
        };
        return (table, true);
    }
    let mut rows = tables
        .first()
        .map(|t| t.rows.iter().skip(1).cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    for t in tables.iter().skip(1) {
        // Where each column of this table goes; a repeated name takes the next match.
        let mut taken = vec![false; head.len()];
        let mut places = Vec::new();
        for h in names(t) {
            let at = (0..head.len()).find(|&i| head[i] == h && !taken[i]);
            let at = at.unwrap_or_else(|| {
                head.push(h.clone());
                taken.push(false);
                head.len() - 1
            });
            taken[at] = true;
            places.push(at);
        }
        for row in t.rows.iter().skip(1) {
            let mut out = vec![Cell::Empty; head.len()];
            for (cell, &at) in row.iter().zip(&places) {
                out[at] = cell.clone();
            }
            rows.push(out);
        }
    }
    let width = head.len();
    rows.iter_mut().for_each(|r| {
        if r.len() < width {
            r.resize(width, Cell::Empty);
        }
    });
    rows.insert(
        0,
        head.into_iter().map(|h| parse_scalar(&h, false)).collect(),
    );
    let table = Table {
        name,
        rows,
        ..Table::default()
    };
    (table, false)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
