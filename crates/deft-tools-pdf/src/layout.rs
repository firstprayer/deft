//! Where text sits on a page, read from the content stream. It gives rows and
//! columns for table extraction; widths are not computed, so a run's start is
//! exact and its end is an estimate.

use crate::num;
use lopdf::{Document, Encoding, Object, ObjectId};
use std::collections::BTreeMap;

/// One piece of text, as drawn by a single operator.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub text: String,
}

type Matrix = [f64; 6];

fn multiply(a: Matrix, b: Matrix) -> Matrix {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

fn matrix(o: &[Object]) -> Option<Matrix> {
    let v: Vec<f64> = o.iter().filter_map(num).collect();
    <[f64; 6]>::try_from(v).ok()
}

fn decode(encoding: Option<&Encoding>, o: &Object, out: &mut String) {
    match o {
        Object::String(bytes, _) => match encoding {
            Some(e) => out.push_str(&Document::decode_text(e, bytes).unwrap_or_default()),
            None => out.push_str(&String::from_utf8_lossy(bytes)),
        },
        Object::Array(items) => {
            for item in items {
                match item {
                    // A large negative kern is a gap between words.
                    Object::Integer(_) | Object::Real(_) => {
                        if num(item).is_some_and(|k| k < -200.0) && !out.ends_with(' ') {
                            out.push(' ');
                        }
                    }
                    other => decode(encoding, other, out),
                }
            }
        }
        _ => {}
    }
}

/// Every run of text on a page, in drawing order.
pub fn runs(doc: &Document, page: ObjectId) -> Vec<Run> {
    let Ok(content) = doc.get_and_decode_page_content(page) else {
        return vec![];
    };
    let fonts = doc.get_page_fonts(page).unwrap_or_default();
    let encodings: BTreeMap<Vec<u8>, Encoding> = fonts
        .iter()
        .filter_map(|(name, f)| Some((name.clone(), f.get_font_encoding(doc).ok()?)))
        .collect();
    let identity: Matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    let mut ctm = identity;
    let mut stack = Vec::new();
    let (mut tm, mut line) = (identity, identity);
    let (mut font, mut size, mut leading) = (Vec::new(), 0.0, 0.0);
    let mut out = Vec::new();
    for op in &content.operations {
        let args = &op.operands;
        match op.operator.as_str() {
            "q" => stack.push(ctm),
            "Q" => ctm = stack.pop().unwrap_or(identity),
            "cm" => {
                if let Some(m) = matrix(args) {
                    ctm = multiply(m, ctm);
                }
            }
            "BT" => {
                tm = identity;
                line = identity;
            }
            "Tf" => {
                if let [Object::Name(n), s] = args.as_slice() {
                    font = n.clone();
                    size = num(s).unwrap_or(0.0);
                }
            }
            "TL" => leading = args.first().and_then(num).unwrap_or(leading),
            "Tm" => {
                if let Some(m) = matrix(args) {
                    tm = m;
                    line = m;
                }
            }
            "Td" | "TD" => {
                if let [x, y] = args.as_slice() {
                    let (x, y) = (num(x).unwrap_or(0.0), num(y).unwrap_or(0.0));
                    if op.operator == "TD" {
                        leading = -y;
                    }
                    line = multiply([1.0, 0.0, 0.0, 1.0, x, y], line);
                    tm = line;
                }
            }
            "T*" | "'" | "\"" | "Tj" | "TJ" => {
                if matches!(op.operator.as_str(), "T*" | "'" | "\"") {
                    line = multiply([1.0, 0.0, 0.0, 1.0, 0.0, -leading], line);
                    tm = line;
                }
                let mut text = String::new();
                if let Some(last) = args.last() {
                    decode(encodings.get(&font), last, &mut text);
                }
                if !text.trim().is_empty() {
                    let m = multiply(tm, ctm);
                    out.push(Run {
                        x: m[4],
                        y: m[5],
                        size: size * m[3].abs().max(m[0].abs()),
                        text: text.trim().to_string(),
                    });
                }
            }
            _ => {}
        }
    }
    out
}

/// Lines of a page, top to bottom, each sorted left to right.
fn rows(mut all: Vec<Run>) -> Vec<Vec<Run>> {
    all.sort_by(|a, b| b.y.total_cmp(&a.y));
    let mut rows: Vec<Vec<Run>> = Vec::new();
    for run in all {
        let tolerance = (run.size * 0.4).max(1.5);
        match rows.last_mut() {
            Some(row) if (row[0].y - run.y).abs() <= tolerance => row.push(run),
            _ => rows.push(vec![run]),
        }
    }
    for row in &mut rows {
        row.sort_by(|a, b| a.x.total_cmp(&b.x));
    }
    rows
}

/// Rows of cells from a page. Column starts are the left edges that recur
/// down the page, so plain paragraphs fall into the first column.
pub fn table(doc: &Document, page: ObjectId) -> Vec<Vec<String>> {
    let lines = rows(runs(doc, page));
    let mut starts: Vec<(f64, usize)> = Vec::new();
    for run in lines.iter().flatten() {
        match starts.iter_mut().find(|(x, _)| (*x - run.x).abs() < 6.0) {
            Some(s) => s.1 += 1,
            None => starts.push((run.x, 1)),
        }
    }
    let needed = (lines.len() / 4).max(2);
    let mut columns: Vec<f64> = starts
        .iter()
        .filter(|(_, n)| *n >= needed)
        .map(|(x, _)| *x)
        .collect();
    columns.sort_by(f64::total_cmp);
    if columns.is_empty() {
        columns.push(0.0);
    }
    lines
        .iter()
        .map(|row| {
            let mut cells = vec![String::new(); columns.len()];
            for run in row {
                let at = columns.iter().rposition(|c| *c <= run.x + 6.0).unwrap_or(0);
                if !cells[at].is_empty() {
                    cells[at].push(' ');
                }
                cells[at].push_str(&run.text);
            }
            cells
        })
        .collect()
}

/// Regions, in points from the top-left of the page, of the runs holding `text`.
pub fn find(doc: &Document, page: ObjectId, text: &str) -> Vec<[f64; 4]> {
    let needle = text.to_lowercase();
    let (_, height) = crate::display_size(doc, page);
    let [x0, y0, ..] = crate::page_box(doc, page);
    runs(doc, page)
        .into_iter()
        .filter(|r| r.text.to_lowercase().contains(&needle))
        .map(|r| {
            // Widths are unknown, so allow a generous 0.65 em per character.
            let width = r.text.chars().count() as f64 * r.size * 0.65;
            let top = height - (r.y - y0) - r.size;
            [r.x - x0 - 1.0, top - 1.0, width + 2.0, r.size * 1.4 + 2.0]
        })
        .collect()
}
