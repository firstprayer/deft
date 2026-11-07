//! Rows and columns from where text sits on a page, for table extraction.

use crate::text::PageText;
use std::collections::BTreeMap;

/// One piece of text, as drawn by a single operator. `y` is its baseline, measured
/// from the bottom of the page as shown.
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub text: String,
    pub width: f64,
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
                // Some writers draw every letter on its own: touching pieces
                // are one word, and only a visible gap is a space.
                let touching = run.x - (last.x + last.width) < last.size * 0.12;
                if !touching && !last.text.ends_with(' ') && !run.text.starts_with(' ') {
                    last.text.push(' ');
                }
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

/// The left and right edges of the text on a page.
fn margins(lines: &[Vec<Run>]) -> (f64, f64) {
    let all = || lines.iter().flatten();
    let left = all().map(|r| r.x).fold(f64::MAX, f64::min);
    let right = all().map(|r| r.x + r.width).fold(f64::MIN, f64::max);
    (left, right)
}

fn is_caption(line: &[Run]) -> bool {
    line[0].text.starts_with("Table ") || line[0].text.starts_with("Figure ")
}

/// Prose or a caption, which ends a table. A long line alone is prose when it
/// starts at the margin or fills the width; a centred one is a group label.
fn is_text(line: &[Run], (left, right): (f64, f64)) -> bool {
    let run = &line[0];
    let wide = run.width > (right - left) * 0.7;
    let lone_prose = line.len() < 2 && chars(line) > 40 && (run.x - left < 4.0 || wide);
    let long_line = line.len() <= 3 && chars(line) > 70 && run.x - left < 4.0;
    is_caption(line) || lone_prose || long_line
}

/// Unbroken stretches of lines that are not prose. A gap much taller than the
/// rows so far ends one too.
fn blocks(lines: &[Vec<Run>]) -> Vec<(usize, usize)> {
    let edges = margins(lines);
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
            && (is_text(line, edges) || lone || step > gap * 2.5)
        {
            found.push((s, i));
            start = None;
        }
        if is_text(line, edges) || lone {
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
    // The names and addresses at the top of a paper line up like a table.
    let authors = all
        .iter()
        .filter(|r| r.text.contains('@') || r.text.contains("University"))
        .count();
    long * 2 < all.len()
        && authors * 4 < all.len()
        && full * 2 >= block.len()
        && letters <= all.len() * 24
        && garbled * 10 < letters
}

fn fill_row(line: &[Run], columns: &[(f64, f64)]) -> Vec<String> {
    let mut row = vec![String::new(); columns.len()];
    for run in line {
        let cell = &mut row[column_of(run, columns)];
        if !cell.is_empty() {
            cell.push(' ');
        }
        cell.push_str(&run.text);
    }
    row
}

/// A sentence leading into a table, such as `Costs follow.` A capital before the
/// full stop is left alone, because headers abbreviate: `Params. Acc.`
fn ends_sentence(line: &[Run]) -> bool {
    let text = line[0].text.trim_end();
    let word = text.rsplit(' ').next().unwrap_or("");
    text.ends_with(':') || (text.ends_with('.') && word.starts_with(char::is_lowercase))
}

/// The line above a table as its header, if it is one. Tight headers arrive as one
/// piece across several columns, so its words must match those columns one to one,
/// and it must reach them all but the row labels. Prose that happens to lie over
/// some of the columns does neither.
fn header_row(line: &[Run], columns: &[(f64, f64)]) -> Option<Vec<String>> {
    if is_caption(line) || (line.len() == 1 && ends_sentence(line)) {
        return None;
    }
    let mut row = vec![String::new(); columns.len()];
    let mut filled = 0;
    let mut spread = false;
    for run in line {
        let under: Vec<usize> = (0..columns.len())
            .filter(|&i| columns[i].0 < run.x + run.width && run.x < columns[i].1)
            .collect();
        let words: Vec<&str> = run.text.split_whitespace().collect();
        if under.len() < 2 {
            let at = under
                .first()
                .copied()
                .unwrap_or_else(|| column_of(run, columns));
            row[at] = run.text.clone();
            filled += 1;
        } else if words.len() == under.len() {
            for (at, word) in under.iter().zip(words) {
                row[*at] = word.to_string();
            }
            filled += under.len();
            spread = true;
        } else {
            return None;
        }
    }
    let enough = !spread || filled + 1 >= columns.len();
    (enough && filled >= 2 && filled * 2 >= columns.len()).then_some(row)
}

fn page_tables(lines: &[Vec<Run>]) -> Vec<Found> {
    let mut out = Vec::new();
    for (start, end) in blocks(lines) {
        // A single cell at either end is a title or a note, not a row.
        let (mut first, mut last) = (start, end);
        while first < last && lines[first].len() < 2 {
            first += 1;
        }
        while first < last && lines[last - 1].len() < 2 {
            last -= 1;
        }
        let columns = spans(&lines[first..last]);
        // A heading straight after the table has too few cells to be a row.
        while columns.len() >= 4 && first < last && lines[last - 1].len() * 2 <= columns.len() {
            last -= 1;
        }
        let block = &lines[first..last];
        if !is_table(block, &columns) {
            continue;
        }
        let mut rows: Vec<Vec<String>> = Vec::new();
        if first > 0
            && lines[first - 1][0].y - lines[first][0].y < lines[first][0].size * 3.0
            && let Some(header) = header_row(&lines[first - 1], &columns)
        {
            rows.push(header);
        }
        for (n, line) in block.iter().enumerate() {
            let spread = (n == 0 && line.len() < columns.len())
                .then(|| header_row(line, &columns))
                .flatten();
            rows.push(spread.unwrap_or_else(|| fill_row(line, &columns)));
        }
        out.push(Found {
            columns: columns.iter().map(|c| c.0).collect(),
            rows,
            at_end: end == lines.len(),
        });
    }
    out
}

/// Each table on a page, kept apart. Unlike `table`, text outside tables is left out.
pub fn tables(page: &PageText) -> Vec<Found> {
    let lines: Vec<Vec<Run>> = rows(page.runs()).iter().map(|l| cells(l)).collect();
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

#[cfg(test)]
#[path = "layout_tests.rs"]
mod tests;
