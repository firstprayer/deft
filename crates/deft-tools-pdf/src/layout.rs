//! Where text sits on a page, read from the content stream. It gives rows and
//! columns for table extraction. Widths come from the font where it lists them,
//! and otherwise are an estimate of half an em per character.

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
    pub width: f64,
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

/// Glyph widths of a simple font, in thousandths of an em.
#[derive(Default)]
struct Widths {
    first: i64,
    glyphs: Vec<f64>,
    /// Composite fonts spend two bytes on a glyph.
    composite: bool,
}

impl Widths {
    fn of(doc: &Document, font: &lopdf::Dictionary) -> Widths {
        let resolve = |o: &Object| match o {
            Object::Reference(id) => doc.get_object(*id).ok().cloned(),
            other => Some(other.clone()),
        };
        let first = font
            .get(b"FirstChar")
            .ok()
            .and_then(resolve)
            .and_then(|o| o.as_i64().ok());
        let glyphs = font.get(b"Widths").ok().and_then(resolve);
        let glyphs = glyphs.as_ref().and_then(|o| o.as_array().ok());
        Widths {
            composite: font
                .get(b"Subtype")
                .is_ok_and(|t| t.as_name().is_ok_and(|n| n == b"Type0")),
            first: first.unwrap_or(0),
            glyphs: glyphs
                .map(|a| {
                    a.iter()
                        .filter_map(|o| resolve(o).as_ref().and_then(num))
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    /// The advance of these bytes, in ems.
    fn advance(&self, bytes: &[u8]) -> f64 {
        if self.composite {
            return bytes.len() as f64 * 0.25;
        }
        if self.glyphs.is_empty() {
            return bytes.len() as f64 * 0.5;
        }
        let glyph = |b: &u8| {
            let at = i64::from(*b) - self.first;
            usize::try_from(at)
                .ok()
                .and_then(|i| self.glyphs.get(i))
                .copied()
        };
        bytes.iter().map(|b| glyph(b).unwrap_or(500.0)).sum::<f64>() / 1000.0
    }
}

/// A kern this wide, in thousandths of an em, is a gap between words.
const WORD_GAP: f64 = 200.0;
/// One this wide separates cells: LaTeX writes a whole table row in one operator.
const CELL_GAP: f64 = 700.0;

/// Text drawn by one operator, split at wide gaps, as (start, text, width) in ems,
/// and the total advance.
fn pieces(
    encoding: Option<&Encoding>,
    widths: Option<&Widths>,
    o: &Object,
) -> (Vec<(f64, String, f64)>, f64) {
    let mut out = Vec::new();
    let (mut x, mut start) = (0.0, 0.0);
    let mut text = String::new();
    let items = match o {
        Object::Array(items) => items.as_slice(),
        other => std::slice::from_ref(other),
    };
    for item in items {
        match item {
            Object::String(bytes, _) => {
                match encoding {
                    Some(e) => text.push_str(&Document::decode_text(e, bytes).unwrap_or_default()),
                    None => text.push_str(&String::from_utf8_lossy(bytes)),
                }
                x += widths.map_or(bytes.len() as f64 * 0.5, |w| w.advance(bytes));
            }
            Object::Integer(_) | Object::Real(_) => {
                let kern = num(item).unwrap_or(0.0);
                if kern <= -CELL_GAP {
                    out.push((start, std::mem::take(&mut text), x - start));
                    x -= kern / 1000.0;
                    start = x;
                    continue;
                }
                x -= kern / 1000.0;
                if kern < -WORD_GAP && !text.ends_with(' ') {
                    text.push(' ');
                }
            }
            _ => {}
        }
    }
    out.push((start, text, x - start));
    (out, x)
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
    let widths: BTreeMap<Vec<u8>, Widths> = fonts
        .iter()
        .map(|(name, f)| (name.clone(), Widths::of(doc, f)))
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
                let Some(last) = args.last() else {
                    continue;
                };
                let (found, advance) = pieces(encodings.get(&font), widths.get(&font), last);
                let m = multiply(tm, ctm);
                let scale = size * m[0].abs().max(m[1].abs());
                for (start, text, width) in found {
                    if !text.trim().is_empty() {
                        out.push(Run {
                            x: m[4] + start * scale,
                            y: m[5],
                            size: size * m[3].abs().max(m[0].abs()),
                            text: text.trim().to_string(),
                            width: width * scale,
                        });
                    }
                }
                tm = multiply([1.0, 0.0, 0.0, 1.0, advance * size, 0.0], tm);
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

/// Left edges that recur down these lines, so plain paragraphs fall into the
/// first column.
fn column_starts(lines: &[Vec<Run>]) -> Vec<f64> {
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
    columns
}

fn grid(lines: &[Vec<Run>], columns: &[f64]) -> Vec<Vec<String>> {
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

/// Rows of cells from a page, as one grid.
pub fn table(doc: &Document, page: ObjectId) -> Vec<Vec<String>> {
    let lines = rows(runs(doc, page));
    grid(&lines, &column_starts(&lines))
}

/// A table found on a page, with the left edges of its columns.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub columns: Vec<f64>,
    pub rows: Vec<Vec<String>>,
    /// Its last line is the last thing on the page, so it may go on overleaf.
    pub at_end: bool,
}

/// Joins pieces of one phrase, and only a clear gap starts a new cell.
fn cells(line: &[Run]) -> Vec<Run> {
    let mut out: Vec<Run> = Vec::new();
    for run in line {
        match out.last_mut() {
            Some(last) if run.x - (last.x + last.width) < last.size * 0.7 => {
                last.text.push(' ');
                last.text.push_str(&run.text);
                last.width = run.x + run.width - last.x;
            }
            _ => out.push(run.clone()),
        }
    }
    out
}

fn chars(line: &[Run]) -> usize {
    line.iter().map(|r| r.text.chars().count()).sum()
}

/// Prose or a caption, which ends a table. A short line alone can be a group
/// label inside one.
fn is_text(line: &[Run]) -> bool {
    let caption = line[0].text.starts_with("Table ") || line[0].text.starts_with("Figure ");
    caption || (line.len() < 2 && chars(line) > 40) || (line.len() <= 3 && chars(line) > 70)
}

/// Unbroken stretches of lines that are not prose. A gap much taller than the
/// rows so far ends one too.
fn blocks(lines: &[Vec<Run>]) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut start = None;
    let mut gap = f64::MAX;
    for (i, line) in lines.iter().enumerate() {
        let step = if i > 0 {
            lines[i - 1][0].y - line[0].y
        } else {
            0.0
        };
        // A short line between rows is a group label only if the rows match.
        let lone = line.len() < 2
            && lines.get(i + 1).map(Vec::len) != lines.get(i.wrapping_sub(1)).map(Vec::len);
        if let Some(s) = start
            && (is_text(line) || lone || step > gap * 3.0)
        {
            found.push((s, i));
            start = None;
        }
        if is_text(line) || lone {
            continue;
        }
        match start {
            None => {
                start = Some(i);
                gap = f64::MAX;
            }
            Some(s) if s + 1 == i => gap = step,
            Some(_) => gap = gap.max(step),
        }
    }
    if let Some(s) = start {
        found.push((s, lines.len()));
    }
    found
}

/// The spans of the columns, from the rows with the usual number of cells. Rows
/// with fewer cells have headers across columns, which would hide the gaps.
fn spans(block: &[Vec<Run>]) -> Vec<(f64, f64)> {
    let mut counts: BTreeMap<usize, usize> = BTreeMap::new();
    for line in block.iter().filter(|l| l.len() >= 2) {
        *counts.entry(line.len()).or_default() += 1;
    }
    let Some((&usual, _)) = counts.iter().max_by_key(|(n, c)| (**c, **n)) else {
        return vec![];
    };
    let mut all: Vec<(f64, f64)> = block
        .iter()
        .filter(|l| l.len() == usual)
        .flatten()
        .map(|r| (r.x, r.x + r.width))
        .collect();
    all.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for (start, end) in all {
        match merged.last_mut() {
            Some(last) if start <= last.1 + 1.0 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

fn column_of(run: &Run, spans: &[(f64, f64)]) -> usize {
    let middle = run.x + run.width / 2.0;
    let distance = |s: &(f64, f64)| {
        if middle < s.0 {
            s.0 - middle
        } else {
            (middle - s.1).max(0.0)
        }
    };
    (0..spans.len())
        .min_by(|a, b| distance(&spans[*a]).total_cmp(&distance(&spans[*b])))
        .unwrap_or(0)
}

/// A display equation: pieces of a formula, often with a number such as `(13)`.
fn is_equation(block: &[Vec<Run>]) -> bool {
    let numbered = |r: &Run| {
        let inner = r.text.strip_prefix('(').and_then(|t| t.strip_suffix(')'));
        inner.is_some_and(|t| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit()))
    };
    let formula = |r: &Run| r.text.contains(['∑', '∫', '∏']);
    block.iter().flatten().any(formula) || block.iter().any(|l| l.last().is_some_and(numbered))
}

/// Whether these lines read as a table: most rows fill the columns, and cells are
/// short. Two columns of prose line up too, but their cells are long.
fn is_table(block: &[Vec<Run>], spans: &[(f64, f64)]) -> bool {
    if block.len() < 3 || spans.len() < 2 || is_equation(block) {
        return false;
    }
    let all: Vec<&Run> = block.iter().flatten().collect();
    let long = all.iter().filter(|r| r.text.chars().count() > 40).count();
    let letters: usize = all.iter().map(|r| r.text.chars().count()).sum();
    // A font with no usable encoding decodes to replacement characters.
    let garbled = all
        .iter()
        .flat_map(|r| r.text.chars())
        .filter(|c| *c == '\u{FFFD}')
        .count();
    let full = block.iter().filter(|l| l.len() == spans.len()).count();
    long * 2 < all.len()
        && full * 2 >= block.len()
        && letters <= all.len() * 24
        && garbled * 10 < letters
}

fn page_tables(lines: &[Vec<Run>]) -> Vec<Found> {
    let mut out = Vec::new();
    for (start, end) in blocks(lines) {
        // A single cell at either end is a title or a note, not a row.
        let mut block = &lines[start..end];
        while block.first().is_some_and(|l| l.len() < 2) {
            block = &block[1..];
        }
        while block.last().is_some_and(|l| l.len() < 2) {
            block = &block[..block.len() - 1];
        }
        let columns = spans(block);
        if !is_table(block, &columns) {
            continue;
        }
        let rows = block
            .iter()
            .map(|line| {
                let mut row = vec![String::new(); columns.len()];
                for run in line {
                    let cell = &mut row[column_of(run, &columns)];
                    if !cell.is_empty() {
                        cell.push(' ');
                    }
                    cell.push_str(&run.text);
                }
                row
            })
            .collect();
        out.push(Found {
            columns: columns.iter().map(|c| c.0).collect(),
            rows,
            at_end: end == lines.len(),
        });
    }
    out
}

/// Each table on a page, kept apart. Unlike `table`, text outside tables is left out.
pub fn tables(doc: &Document, page: ObjectId) -> Vec<Found> {
    let lines: Vec<Vec<Run>> = rows(runs(doc, page)).iter().map(|l| cells(l)).collect();
    page_tables(&lines)
}

/// Joins tables that carry on from one page to the next: the same columns, with
/// the first ending the page. A repeated header is dropped.
pub fn join_pages(pages: Vec<Vec<Found>>) -> Vec<Vec<Vec<String>>> {
    let mut out: Vec<Found> = Vec::new();
    for (n, page) in pages.into_iter().enumerate() {
        for (i, mut table) in page.into_iter().enumerate() {
            let same = |a: &Found| {
                a.columns.len() == table.columns.len()
                    && a.columns
                        .iter()
                        .zip(&table.columns)
                        .all(|(x, y)| (x - y).abs() < 6.0)
            };
            match out.last_mut() {
                Some(last) if n > 0 && i == 0 && last.at_end && same(last) => {
                    if last.rows.first() == table.rows.first() {
                        table.rows.remove(0);
                    }
                    last.rows.append(&mut table.rows);
                    last.at_end = table.at_end;
                }
                _ => out.push(table),
            }
        }
    }
    out.into_iter().map(|t| t.rows).collect()
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

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
