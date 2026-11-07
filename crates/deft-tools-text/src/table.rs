//! Tables for the PDF: column widths that leave room for the longest word,
//! words broken where they still do not fit, merged cells, and tables
//! written as raw HTML in Markdown.

/// Width of text in units of about half an em: wide scripts count double.
fn char_units(c: char) -> f64 {
    let wide = matches!(c as u32,
        0x1100..=0x115F | 0x2E80..=0xA4CF | 0xAC00..=0xD7A3 | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F | 0xFF00..=0xFF60 | 0xFFE0..=0xFFE6 | 0x1F300..=0x1FAFF
        | 0x20000..=0x3FFFD);
    if wide { 2.0 } else { 1.0 }
}

/// The longest word and the whole text of a cell, in units.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Measure {
    pub word: f64,
    pub total: f64,
}

pub(crate) fn measure(text: &str) -> Measure {
    let mut m = Measure::default();
    for line in text.split('\n') {
        let mut total = 0.0;
        for w in line.split_whitespace() {
            let units: f64 = w.chars().map(char_units).sum();
            m.word = m.word.max(units);
            total += units + 1.0;
        }
        m.total = m.total.max(total);
    }
    m
}

/// How a table's columns are laid out.
#[derive(Debug, PartialEq)]
pub(crate) struct Columns {
    /// Typst `columns:` value.
    pub spec: String,
    /// Longest run of units each column can hold without breaking.
    pub caps: Vec<f64>,
}

/// Points per unit: generous, so that an estimate errs towards breaking.
const UNIT: f64 = 0.6;
/// Space a cell's inset takes, in points.
const INSET: f64 = 12.0;

/// Column widths for a table `width` points wide. `cells` are the measures
/// of cells spanning one column, as (column, measure).
pub(crate) fn columns(n: usize, cells: &[(usize, Measure)], width: f64, font: f64) -> Columns {
    let n = n.max(1);
    let unit = UNIT * font;
    let available = (width - INSET * n as f64).max(n as f64 * unit) / unit;
    let mut low = vec![1.0f64; n];
    let mut high = vec![1.0f64; n];
    for (c, m) in cells {
        if *c < n {
            // A very long word, such as a URL, is broken rather than given room.
            low[*c] = low[*c].max(m.word.min(24.0));
            high[*c] = high[*c].max(m.total);
        }
    }
    let sum = |v: &[f64]| v.iter().sum::<f64>();
    if sum(&high) <= available {
        return Columns {
            spec: format!("({})", vec!["auto"; n].join(", ")),
            caps: high.iter().map(|_| available).collect(),
        };
    }
    let widths: Vec<f64> = if sum(&low) >= available {
        // Too narrow for every word: short words keep their room, and the
        // long ones are broken.
        let floor: Vec<f64> = low.iter().map(|w| w.min(8.0)).collect();
        if sum(&floor) >= available {
            low.iter().map(|w| w * available / sum(&low)).collect()
        } else {
            let k = (available - sum(&floor)) / (sum(&low) - sum(&floor));
            floor
                .iter()
                .zip(&low)
                .map(|(f, l)| f + (l - f) * k)
                .collect()
        }
    } else {
        let spare = available - sum(&low);
        let wants: Vec<f64> = low.iter().zip(&high).map(|(l, h)| h - l).collect();
        let total_want = sum(&wants).max(1e-9);
        low.iter()
            .zip(&wants)
            .map(|(l, w)| l + spare * w / total_want)
            .collect()
    };
    Columns {
        spec: format!(
            "({})",
            widths
                .iter()
                // A column's width includes its inset.
                .map(|w| format!("{:.1}fr", w.max(0.5) * unit + INSET))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        caps: widths.iter().map(|w| w.floor().max(2.0)).collect(),
    }
}

/// Text with zero-width spaces where a word longer than `cap` units may
/// break: after punctuation inside it, and wherever it would still overflow.
pub(crate) fn break_long_words(text: &str, cap: f64) -> String {
    let mut out = String::with_capacity(text.len());
    for (i, word) in text.split(' ').enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let units: f64 = word.chars().map(char_units).sum();
        if units <= cap {
            out.push_str(word);
            continue;
        }
        let mut run = 0.0;
        for c in word.chars() {
            let u = char_units(c);
            if run + u > cap && run > 0.0 {
                out.push('\u{200B}');
                run = 0.0;
            }
            out.push(c);
            run += u;
            if matches!(
                c,
                '_' | '/' | '-' | '.' | ':' | ',' | ';' | '=' | '&' | '?' | '|'
            ) {
                out.push('\u{200B}');
                run = 0.0;
            }
        }
    }
    out
}

/// A cell placed on the grid.
pub(crate) struct Placed<T> {
    pub x: usize,
    pub y: usize,
    pub colspan: usize,
    pub rowspan: usize,
    pub cell: T,
}

/// Place rows of (colspan, rowspan, cell) on a grid as HTML does, keeping
/// spans inside the table and off each other. Returns the cells and the
/// number of columns.
pub(crate) fn place<T>(rows: Vec<Vec<(usize, usize, T)>>) -> (Vec<Placed<T>>, usize) {
    let height = rows.len();
    let mut taken: Vec<Vec<bool>> = vec![vec![]; height];
    let mut out = Vec::new();
    let occupied =
        |taken: &Vec<Vec<bool>>, y: usize, x: usize| taken[y].get(x).copied().unwrap_or(false);
    for (y, row) in rows.into_iter().enumerate() {
        let mut x = 0;
        for (colspan, rowspan, cell) in row {
            while occupied(&taken, y, x) {
                x += 1;
            }
            let mut cols = 1;
            while cols < colspan.max(1) && !occupied(&taken, y, x + cols) {
                cols += 1;
            }
            let mut down = 1;
            while down < rowspan.max(1)
                && y + down < height
                && (x..x + cols).all(|c| !occupied(&taken, y + down, c))
            {
                down += 1;
            }
            for line in taken.iter_mut().skip(y).take(down) {
                if line.len() < x + cols {
                    line.resize(x + cols, false);
                }
                for slot in &mut line[x..x + cols] {
                    *slot = true;
                }
            }
            out.push(Placed {
                x,
                y,
                colspan: cols,
                rowspan: down,
                cell,
            });
            x += cols;
        }
    }
    let width = taken.iter().map(Vec::len).max().unwrap_or(0);
    (out, width)
}

/// Inline content of an HTML table cell.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Inline {
    Text {
        text: String,
        bold: bool,
        italic: bool,
        code: bool,
        strike: bool,
        link: Option<String>,
    },
    Break,
    Image {
        src: String,
        alt: String,
    },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct HtmlCell {
    pub colspan: usize,
    pub rowspan: usize,
    pub header: bool,
    pub content: Vec<Inline>,
}

impl HtmlCell {
    pub fn text(&self) -> String {
        self.content
            .iter()
            .map(|i| match i {
                Inline::Text { text, .. } => text.clone(),
                Inline::Break => "\n".to_string(),
                Inline::Image { alt, .. } => alt.clone(),
            })
            .collect()
    }
}

fn entity(name: &str) -> Option<char> {
    if let Some(n) = name.strip_prefix("#x").or_else(|| name.strip_prefix("#X")) {
        return u32::from_str_radix(n, 16).ok().and_then(char::from_u32);
    }
    if let Some(n) = name.strip_prefix('#') {
        return n.parse().ok().and_then(char::from_u32);
    }
    Some(match name {
        "amp" => '&',
        "lt" => '<',
        "gt" => '>',
        "quot" => '"',
        "apos" => '\'',
        "nbsp" => '\u{a0}',
        "ndash" => '–',
        "mdash" => '—',
        "hellip" => '…',
        "copy" => '©',
        "reg" => '®',
        "trade" => '™',
        "laquo" => '«',
        "raquo" => '»',
        "middot" => '·',
        "times" => '×',
        "deg" => '°',
        "euro" => '€',
        "rsquo" => '’',
        "lsquo" => '‘',
        "rdquo" => '”',
        "ldquo" => '“',
        "bull" => '•',
        _ => return None,
    })
}

fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find('&') {
        out.push_str(&rest[..at]);
        rest = &rest[at..];
        let end = rest[1..].find(';').map(|e| e + 1).filter(|e| *e <= 12);
        match end.and_then(|e| entity(&rest[1..e]).map(|c| (e, c))) {
            Some((e, c)) => {
                out.push(c);
                rest = &rest[e + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(at) = lower[from..].find(name) {
        let start = from + at;
        from = start + name.len();
        let before = lower[..start].chars().last();
        if !before.is_some_and(char::is_whitespace) {
            continue;
        }
        let rest = tag[from..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else {
            continue;
        };
        let rest = rest.trim_start();
        let value = match rest.chars().next() {
            Some(q @ ('"' | '\'')) => rest[1..].split(q).next().unwrap_or(""),
            _ => rest
                .split(|c: char| c.is_whitespace() || c == '>' || c == '/')
                .next()
                .unwrap_or(""),
        };
        return Some(decode(value));
    }
    None
}

/// The first `<table>` of an HTML block, as rows of cells.
pub(crate) fn html_table(html: &str) -> Option<Vec<Vec<HtmlCell>>> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<table")?;
    let end = lower[start..].find("</table>")? + start;
    let body = &html[start..end];
    let mut rows: Vec<Vec<HtmlCell>> = Vec::new();
    let mut cell: Option<HtmlCell> = None;
    let (mut bold, mut italic, mut code, mut strike) = (0, 0, 0, 0);
    let mut link: Option<String> = None;
    let mut in_head = false;
    let mut rest = body;
    let mut skip_depth = 0;
    while !rest.is_empty() {
        let next = rest.find('<').unwrap_or(rest.len());
        let text = &rest[..next];
        if let Some(c) = cell.as_mut()
            && skip_depth == 0
        {
            let t = decode(&text.split_whitespace().collect::<Vec<_>>().join(" "));
            let spaced = if text.starts_with(char::is_whitespace) && !t.is_empty() {
                format!(" {t}")
            } else {
                t
            };
            let spaced = if text.ends_with(char::is_whitespace) && !text.trim().is_empty() {
                format!("{spaced} ")
            } else {
                spaced
            };
            if !spaced.is_empty() {
                c.content.push(Inline::Text {
                    text: spaced,
                    bold: bold > 0,
                    italic: italic > 0,
                    code: code > 0,
                    strike: strike > 0,
                    link: link.clone(),
                });
            }
        }
        rest = &rest[next..];
        if rest.is_empty() {
            break;
        }
        let close = rest.find('>').map_or(rest.len(), |i| i + 1);
        let tag = &rest[..close];
        rest = &rest[close..];
        let inner = tag.trim_start_matches('<').trim_end_matches('>');
        let closing = inner.starts_with('/');
        let name: String = inner
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        let step = |n: &mut i32| {
            if closing {
                *n = (*n - 1).max(0)
            } else {
                *n += 1
            }
        };
        match name.as_str() {
            "script" | "style" => {
                if closing {
                    skip_depth = (skip_depth - 1).max(0);
                } else {
                    skip_depth += 1;
                }
            }
            "thead" => in_head = !closing,
            "tr" if !closing => rows.push(vec![]),
            "td" | "th" if !closing => {
                if let (Some(done), Some(row)) = (cell.take(), rows.last_mut()) {
                    row.push(done);
                }
                if rows.is_empty() {
                    rows.push(vec![]);
                }
                let span = |a: &str| {
                    attribute(inner, a)
                        .and_then(|v| v.trim().parse::<usize>().ok())
                        .unwrap_or(1)
                        .clamp(1, 1000)
                };
                cell = Some(HtmlCell {
                    colspan: span("colspan"),
                    rowspan: span("rowspan"),
                    header: name == "th" || in_head,
                    content: vec![],
                });
            }
            "td" | "th" | "tr" => {
                if let (Some(done), Some(row)) = (cell.take(), rows.last_mut()) {
                    row.push(done);
                }
            }
            "b" | "strong" => step(&mut bold),
            "i" | "em" => step(&mut italic),
            "code" | "kbd" | "tt" => step(&mut code),
            "s" | "del" | "strike" => step(&mut strike),
            "a" => {
                link = if closing {
                    None
                } else {
                    attribute(inner, "href").filter(|h| !h.is_empty())
                }
            }
            "br" | "p" | "div" | "li" if cell.as_ref().is_some_and(|c| !c.content.is_empty()) => {
                if let Some(c) = cell.as_mut()
                    && (name == "br" || closing)
                {
                    c.content.push(Inline::Break);
                }
            }
            "img" => {
                if let (Some(c), Some(src)) = (cell.as_mut(), attribute(inner, "src")) {
                    c.content.push(Inline::Image {
                        src,
                        alt: attribute(inner, "alt").unwrap_or_default(),
                    });
                }
            }
            _ => {}
        }
    }
    if let (Some(done), Some(row)) = (cell.take(), rows.last_mut()) {
        row.push(done);
    }
    for row in &mut rows {
        for c in row.iter_mut() {
            while matches!(c.content.last(), Some(Inline::Break)) {
                c.content.pop();
            }
        }
    }
    rows.retain(|r| !r.is_empty());
    (!rows.is_empty()).then_some(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_words_break_inside_their_column() {
        assert_eq!(break_long_words("short words", 6.0), "short words");
        let broken = break_long_words(":face_with_spiral_eyes:", 10.0);
        assert!(
            broken.split('\u{200B}').all(|p| p.chars().count() <= 10),
            "{broken}"
        );
        assert_eq!(broken.replace('\u{200B}', ""), ":face_with_spiral_eyes:");
        let russian = break_long_words("Основание", 4.0);
        assert_eq!(russian, "Осно\u{200B}вани\u{200B}е");
    }

    #[test]
    fn columns_make_room_for_words_or_break_them() {
        let m = |c, t: &str| (c, measure(t));
        // Fits: natural widths.
        let small = columns(2, &[m(0, "a b"), m(1, "c")], 400.0, 11.0);
        assert_eq!(small.spec, "(auto, auto)");
        // Too wide: the narrow column keeps its word whole.
        let wide = columns(
            3,
            &[
                m(0, "top"),
                m(1, &"long text ".repeat(40)),
                m(2, ":face_with_spiral_eyes:"),
            ],
            300.0,
            11.0,
        );
        assert!(wide.caps[0] >= 3.0, "{wide:?}");
        assert!(wide.caps[2] >= 10.0, "{wide:?}");
        assert!(wide.spec.contains("fr"), "{wide:?}");
    }

    #[test]
    fn merged_cells_stay_on_the_grid() {
        let rows = vec![
            vec![(13, 1, "title"), (1, 1, "x")],
            vec![(1, 2, "a"), (1, 1, "b")],
            vec![(5, 1, "c")],
        ];
        let (cells, width) = place(rows);
        assert_eq!(width, 14);
        let at: Vec<(usize, usize, usize, usize, &str)> = cells
            .iter()
            .map(|p| (p.x, p.y, p.colspan, p.rowspan, p.cell))
            .collect();
        assert_eq!(at[0], (0, 0, 13, 1, "title"));
        assert_eq!(at[1], (13, 0, 1, 1, "x"));
        assert_eq!(at[2], (0, 1, 1, 2, "a"));
        // Row 3 starts after the cell from above, and its span is cut there.
        assert_eq!(at[4], (1, 2, 5, 1, "c"));
    }

    #[test]
    fn html_tables_are_read_with_spans() {
        let html = "<table><thead><tr><th colspan=\"2\">Head &amp; more</th></tr></thead><tr><td rowspan=2><b>bold</b> <a href=\"https://a.example\">link</a></td><td>x<br>y</td></tr><tr><td><img src=\"p.png\" alt=\"pic\"></td></tr></table>";
        let rows = html_table(html).unwrap();
        assert_eq!(rows.len(), 3);
        assert_eq!((rows[0][0].colspan, rows[0][0].header), (2, true));
        assert_eq!(rows[0][0].text(), "Head & more");
        assert_eq!(rows[1][0].rowspan, 2);
        assert_eq!(rows[1][1].text(), "x\ny");
        assert!(matches!(
            &rows[1][0].content[0],
            Inline::Text { bold: true, .. }
        ));
        assert!(matches!(&rows[2][0].content[0], Inline::Image { src, .. } if src == "p.png"));
    }
}
