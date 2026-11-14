//! Blocks to Markdown text, and Markdown text to blocks.

use crate::{Block, Document, Merge, Span};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::ops::Range;

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(c, '*' | '_' | '`' | '[' | ']' | '<' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// A file name as a Markdown link target: spaces and brackets would end it.
pub fn link_name(name: &str) -> String {
    name.replace(' ', "%20")
        .replace('(', "%28")
        .replace(')', "%29")
}

/// Neighbours with the same style as one span, so that emphasis marks do
/// not meet in the middle of a word.
fn joined(spans: &[Span]) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    for s in spans {
        let plain = |x: &Span| x.image.is_none() && !x.note;
        match out.last_mut() {
            Some(last)
                if plain(last)
                    && plain(s)
                    && (last.bold, last.italic, last.code, last.strike, &last.link)
                        == (s.bold, s.italic, s.code, s.strike, &s.link) =>
            {
                last.text.push_str(&s.text)
            }
            _ => out.push(s.clone()),
        }
    }
    out
}

/// Whether a span carries emphasis that Markdown writes with delimiters.
fn emphasized(s: &Span) -> bool {
    s.image.is_none() && !s.note && !s.code && (s.bold || s.italic || s.strike)
}

/// Text written as it is, its line breaks kept as Markdown breaks.
fn plain_text(text: &str) -> String {
    escape(text).replace('\n', "  \n")
}

/// A code span, fenced with more backticks than the code holds in a row.
fn code_span(text: &str) -> String {
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest + 1);
    let pad = if text.starts_with('`') || text.ends_with('`') {
        " "
    } else {
        ""
    };
    format!("{fence}{pad}{text}{pad}{fence}")
}

/// Punctuation as CommonMark counts it: ASCII punctuation and the Unicode
/// punctuation and symbol blocks common in documents.
pub(crate) fn is_punctuation(c: char) -> bool {
    // Symbols such as × or → count as punctuation for newer readers but as
    // letters for older ones, so only true punctuation is taken here: what
    // passes for both.
    c.is_ascii_punctuation()
        || matches!(c as u32,
            0x00A1 | 0x00A7 | 0x00AB | 0x00B6 | 0x00B7 | 0x00BB | 0x00BF
            | 0x2010..=0x2027 | 0x2030..=0x2043 | 0x2045..=0x2051 | 0x2053..=0x205E
            | 0x3001..=0x3003 | 0x3008..=0x3011 | 0x3014..=0x301F | 0x30FB
            | 0xFE10..=0xFE19 | 0xFE30..=0xFE52 | 0xFE54..=0xFE61 | 0xFE63 | 0xFE68 | 0xFE6A | 0xFE6B
            | 0xFF01..=0xFF03 | 0xFF05..=0xFF0A | 0xFF0C..=0xFF0F | 0xFF1A | 0xFF1B | 0xFF1F
            | 0xFF20 | 0xFF3B..=0xFF3D | 0xFF3F | 0xFF5B | 0xFF5D | 0xFF5F..=0xFF65)
}

/// Whether the delimiters around `body` in `md` open and close emphasis for
/// every CommonMark reader, old or new: the flanking rules, with only true
/// punctuation counted as punctuation.
fn flanks(md: &str, body: &Range<usize>) -> bool {
    let marks = |c: char| matches!(c, '*' | '~');
    let open = md[..body.start]
        .chars()
        .rev()
        .take_while(|c| marks(*c))
        .count();
    let close = md[body.end..].chars().take_while(|c| marks(*c)).count();
    if open == 0 || close == 0 {
        return true;
    }
    let before = md[..body.start].chars().rev().nth(open);
    let after = md[body.end..].chars().nth(close);
    let (first, last) = (
        md[body.clone()].chars().next(),
        md[body.clone()].chars().last(),
    );
    let space = |c: Option<char>| c.is_none_or(char::is_whitespace);
    let punct = |c: Option<char>| c.is_some_and(is_punctuation);
    let left = !space(first) && (!punct(first) || space(before) || punct(before));
    let right = !space(last) && (!punct(last) || space(after) || punct(after));
    left && right
}

/// One span as Markdown, and where its own text sits in the result.
/// `level` says how much of its emphasis was given up so that a reader
/// sees it: 1 moves punctuation at its ends outside the delimiters, 2 drops
/// the emphasis.
fn span_markdown(s: &Span, level: u8) -> (String, Range<usize>) {
    if let Some(name) = &s.image {
        let picture = format!("![{}]({})", escape(&s.text), link_name(name));
        let out = match &s.link {
            Some(link) => format!("[{picture}]({link})"),
            None => picture,
        };
        return (out, 0..0);
    }
    if s.note {
        // `[1]` is a reference and `[1]:` starts the note.
        return (s.text.replacen('[', "[^", 1), 0..0);
    }
    // Whitespace at the ends goes outside the delimiters, or they do not
    // count: Word runs often end in a space, a tab or a no-break space.
    let mut text = s.text.trim_matches(char::is_whitespace);
    if text.is_empty() {
        return (plain_text(&s.text), 0..0);
    }
    let at = s.text.len() - s.text.trim_start_matches(char::is_whitespace).len();
    let mut lead = s.text[..at].to_string();
    let mut trail = s.text[at + text.len()..].to_string();
    let emphasis = emphasized(s) && level < 2;
    if emphasis && level == 1 {
        // A delimiter between punctuation and a letter is not one.
        let inner = text.trim_matches(is_punctuation);
        let start = text.len() - text.trim_start_matches(is_punctuation).len();
        lead.push_str(&text[..start]);
        trail.insert_str(0, &text[start + inner.len()..]);
        text = inner;
    }
    let body = if s.code {
        code_span(text)
    } else {
        plain_text(text)
    };
    let mut open = String::new();
    if emphasis && !text.is_empty() {
        if s.strike {
            open.push_str("~~");
        }
        open.push_str(match (s.bold, s.italic) {
            (true, true) => "***",
            (true, false) => "**",
            (false, true) => "*",
            _ => "",
        });
    }
    let close: String = open.chars().rev().collect();
    let mut range = open.len()..open.len() + body.len();
    let mut out = format!("{open}{body}{close}");
    if let Some(link) = &s.link {
        out = format!("[{out}]({link})");
        range = range.start + 1..range.end + 1;
    }
    let lead = plain_text(&lead);
    let out = format!("{lead}{out}{}", plain_text(&trail));
    (out, lead.len() + range.start..lead.len() + range.end)
}

/// The spans, by index, that a CommonMark reader would not style as `spans`
/// says: a delimiter beside punctuation, or two delimiters that meet, leave
/// asterisks in the text.
fn misread(md: &str, parts: &[(usize, Range<usize>)], spans: &[Span]) -> Vec<usize> {
    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES;
    let (mut bold, mut italic, mut strike, mut image) = (0i32, 0i32, 0i32, 0i32);
    let mut wrong = Vec::new();
    for (event, range) in Parser::new_ext(md, options).into_offset_iter() {
        let code = match event {
            Event::Start(Tag::Strong) => {
                bold += 1;
                continue;
            }
            Event::End(TagEnd::Strong) => {
                bold -= 1;
                continue;
            }
            Event::Start(Tag::Emphasis) => {
                italic += 1;
                continue;
            }
            Event::End(TagEnd::Emphasis) => {
                italic -= 1;
                continue;
            }
            Event::Start(Tag::Strikethrough) => {
                strike += 1;
                continue;
            }
            Event::End(TagEnd::Strikethrough) => {
                strike -= 1;
                continue;
            }
            Event::Start(Tag::Image { .. }) => {
                image += 1;
                continue;
            }
            Event::End(TagEnd::Image) => {
                image -= 1;
                continue;
            }
            Event::Text(_) if image == 0 => false,
            Event::Code(_) if image == 0 => true,
            _ => continue,
        };
        for (i, body) in parts {
            if body.start >= range.end || range.start >= body.end || wrong.contains(i) {
                continue;
            }
            let s = &spans[*i];
            if (bold > 0, italic > 0, strike > 0, code) != (s.bold, s.italic, s.strike, s.code) {
                wrong.push(*i);
            }
        }
    }
    wrong
}

fn inline(spans: &[Span]) -> String {
    // Emphasis on whitespace alone shows nothing and only gets in the way.
    let cleaned: Vec<Span> = spans
        .iter()
        .map(|s| {
            let mut s = s.clone();
            if emphasized(&s) && s.text.trim().is_empty() {
                (s.bold, s.italic, s.strike) = (false, false, false);
            }
            s
        })
        .collect();
    let spans = joined(&cleaned);
    let mut levels = vec![0u8; spans.len()];
    loop {
        let mut out = String::new();
        // Each span's text in `out`, with the style a reader must see there.
        let mut parts = Vec::new();
        let mut meant = Vec::new();
        for (i, s) in spans.iter().enumerate() {
            let (md, body) = span_markdown(s, levels[i]);
            let mut m = s.clone();
            if levels[i] >= 2 {
                (m.bold, m.italic, m.strike) = (false, false, false);
            }
            meant.push(m);
            if !body.is_empty() {
                parts.push((i, out.len() + body.start..out.len() + body.end));
            }
            out.push_str(&md);
        }
        if !spans.iter().any(emphasized) {
            return out;
        }
        // Each span a reader would get wrong gives up a little more of its
        // emphasis, until the text reads as meant.
        let mut changed = false;
        let mut wrong = misread(&out, &parts, &meant);
        for (i, body) in &parts {
            if emphasized(&meant[*i]) && !flanks(&out, body) && !wrong.contains(i) {
                wrong.push(*i);
            }
        }
        for i in wrong {
            if levels[i] < 2 {
                levels[i] += 1;
                changed = true;
            }
        }
        if !changed {
            return out;
        }
    }
}

fn table(rows: &[Vec<Vec<Span>>], out: &mut String) {
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    if cols == 0 {
        return;
    }
    let cell = |row: &Vec<Vec<Span>>, i: usize| {
        row.get(i)
            .map(|c| {
                // A cell's own lines stay lines.
                inline(c)
                    .replace('|', "\\|")
                    .replace("  \n", "<br>")
                    .replace('\n', "<br>")
            })
            .unwrap_or_default()
    };
    for (r, row) in rows.iter().enumerate() {
        let line: Vec<String> = (0..cols).map(|i| cell(row, i)).collect();
        out.push_str(&format!("| {} |\n", line.join(" | ")));
        if r == 0 {
            out.push_str(&format!("|{}\n", " --- |".repeat(cols)));
        }
    }
}

/// A line of a paragraph that Markdown reads as a paragraph line. Indents are
/// layout in a document but make code in Markdown, and a line that starts
/// like a heading, list, quote, rule or fence would become one.
fn line_start(line: &str) -> String {
    let line = line.trim_start_matches([' ', '\t']);
    let bytes = line.as_bytes();
    let spaced = |at: usize| bytes.get(at).is_none_or(|b| matches!(b, b' ' | b'\t'));
    let hashes = line.len() - line.trim_start_matches('#').len();
    let digits = line.len() - line.trim_start_matches(|c: char| c.is_ascii_digit()).len();
    let only = |set: &[char]| {
        !line.trim_end().is_empty() && line.trim_end().chars().all(|c| set.contains(&c))
    };
    let escape_at = if (1..=6).contains(&hashes) && spaced(hashes)
        || line.starts_with('>')
        || line.starts_with("~~~")
        || (line.starts_with(['-', '+', '*']) && spaced(1))
        || only(&['-', ' ', '\t'])
        || only(&['=', ' ', '\t'])
        || only(&['_', ' ', '\t'])
    {
        Some(0)
    } else if (1..=9).contains(&digits)
        && matches!(bytes.get(digits), Some(b'.' | b')'))
        && spaced(digits + 1)
    {
        Some(digits)
    } else {
        None
    };
    match escape_at {
        Some(at) => format!("{}\\{}", &line[..at], &line[at..]),
        None => line.to_string(),
    }
}

/// `in_item` keeps a nested list against its item, which keeps the list tight.
fn write(blocks: &[Block], prefix: &str, in_item: bool, html: bool, out: &mut String) {
    for (i, b) in blocks.iter().enumerate() {
        if i > 0 && !(in_item && matches!(b, Block::List { .. })) {
            out.push_str(prefix.trim_end());
            out.push('\n');
        }
        match b {
            Block::Heading(level, spans) => {
                out.push_str(prefix);
                out.push_str(&"#".repeat((*level).clamp(1, 6) as usize));
                out.push(' ');
                out.push_str(&inline(spans).replace('\n', " "));
                out.push('\n');
            }
            Block::Paragraph(spans) => {
                let text = inline(spans);
                for line in text.lines() {
                    out.push_str(prefix);
                    out.push_str(&line_start(line));
                    out.push('\n');
                }
            }
            Block::List {
                ordered,
                start,
                items,
            } => {
                // Markdown joins two lists of the same kind into one.
                if i > 0 && matches!(blocks[i - 1], Block::List { .. }) {
                    out.push_str(prefix);
                    out.push_str("<!-- -->\n\n");
                }
                for (n, item) in items.iter().enumerate() {
                    let marker = if *ordered {
                        format!("{}. ", *start + n as u64)
                    } else {
                        "- ".to_string()
                    };
                    let pad = " ".repeat(marker.len());
                    let mut body = String::new();
                    write(item, "", true, html, &mut body);
                    for (l, line) in body.lines().enumerate() {
                        out.push_str(prefix);
                        out.push_str(if l == 0 { &marker } else { &pad });
                        out.push_str(line);
                        out.push('\n');
                    }
                }
            }
            Block::Quote(inner) => write(inner, &format!("{prefix}> "), false, html, out),
            Block::Code(code) => {
                out.push_str(&format!("{prefix}```\n"));
                for line in code.lines() {
                    out.push_str(prefix);
                    out.push_str(line);
                    out.push('\n');
                }
                out.push_str(&format!("{prefix}```\n"));
            }
            Block::Table(rows, merges)
                if html
                    && (!merges.is_empty()
                        || rows
                            .iter()
                            .flatten()
                            .flatten()
                            .any(|s| s.text.contains('\n'))) =>
            {
                for line in html_table(rows, merges).lines() {
                    out.push_str(prefix);
                    out.push_str(line);
                    out.push('\n');
                }
            }
            Block::Table(rows, _) => table(rows, out),
            Block::Rule => out.push_str("---\n"),
            Block::PageBreak => out.push_str("<!-- page break -->\n"),
        }
    }
}

/// Render a document as Markdown.
pub fn render(doc: &Document) -> String {
    let mut out = String::new();
    write(&doc.blocks, "", false, false, &mut out);
    out
}

/// Markdown for a page or a PDF: a table with merged cells is written as
/// HTML, which can say so. Plain Markdown pads such a table instead.
pub fn render_for_layout(doc: &Document) -> String {
    let mut out = String::new();
    write(&doc.blocks, "", false, true, &mut out);
    out
}

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn html_inline(spans: &[Span]) -> String {
    let mut out = String::new();
    for s in spans {
        if let Some(name) = &s.image {
            out.push_str(&format!(
                "<img src=\"{}\" alt=\"{}\">",
                html_escape(&link_name(name)),
                html_escape(&s.text)
            ));
            continue;
        }
        let mut body = html_escape(&s.text).replace('\n', "<br>");
        for (on, tag) in [
            (s.code, "code"),
            (s.strike, "del"),
            (s.italic, "em"),
            (s.bold, "strong"),
        ] {
            if on {
                body = format!("<{tag}>{body}</{tag}>");
            }
        }
        if let Some(link) = &s.link {
            body = format!("<a href=\"{}\">{body}</a>", html_escape(link));
        }
        out.push_str(&body);
    }
    out
}

/// A table as HTML on one line per row, with no blank line, so that
/// Markdown keeps it as one HTML block.
fn html_table(rows: &[Vec<Vec<Span>>], merges: &[Merge]) -> String {
    let mut out = String::from("<table>\n");
    for (r, row) in rows.iter().enumerate() {
        out.push_str("<tr>");
        for (c, cell) in row.iter().enumerate() {
            let covered = merges.iter().any(|m| {
                (m.row..m.row + m.rows).contains(&r)
                    && (m.col..m.col + m.cols).contains(&c)
                    && (m.row, m.col) != (r, c)
            });
            if covered {
                continue;
            }
            // A table of one row, such as a box around a letter, has no header.
            let tag = if r == 0 && rows.len() > 1 { "th" } else { "td" };
            let mut attrs = String::new();
            if let Some(m) = merges.iter().find(|m| (m.row, m.col) == (r, c)) {
                if m.cols > 1 {
                    attrs.push_str(&format!(" colspan=\"{}\"", m.cols));
                }
                if m.rows > 1 {
                    attrs.push_str(&format!(" rowspan=\"{}\"", m.rows));
                }
            }
            out.push_str(&format!("<{tag}{attrs}>{}</{tag}>", html_inline(cell)));
        }
        out.push_str("</tr>\n");
    }
    out.push_str("</table>\n");
    out
}

#[derive(Default)]
struct Style {
    bold: u32,
    italic: u32,
    strike: u32,
    link: Option<String>,
}

fn span(text: &str, style: &Style, code: bool) -> Span {
    Span {
        text: text.to_string(),
        bold: style.bold > 0,
        italic: style.italic > 0,
        strike: style.strike > 0,
        code,
        link: style.link.clone(),
        ..Default::default()
    }
}

enum Frame {
    Top(Vec<Block>),
    Quote(Vec<Block>),
    List(Option<u64>, Vec<Vec<Block>>),
    Item(Vec<Block>),
    Note(String, Vec<Block>),
}

/// One heading of a Markdown file.
#[derive(Clone, Debug, PartialEq)]
pub struct Heading {
    pub level: usize,
    /// Text for a link label, keeping code spans.
    pub label: String,
    /// Text alone, which is what an anchor is made from.
    pub plain: String,
    /// First and last line, counted from 0.
    pub line: usize,
    pub end_line: usize,
}

/// The headings of a file, found by parsing, so that fences, underlined
/// headings and front matter are all read as a renderer reads them.
pub fn headings(md: &str) -> Vec<Heading> {
    // The parser's own metadata option also takes a "---" rule followed by
    // text anywhere in the file for front matter, and skips what follows.
    let md = &without_front_matter(md);
    let options = Options::ENABLE_TABLES;
    let line_of = |at: usize| md[..at].matches('\n').count();
    let mut out: Vec<Heading> = Vec::new();
    let mut open = false;
    for (event, range) in Parser::new_ext(md, options).into_offset_iter() {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                open = true;
                out.push(Heading {
                    level: level as usize,
                    label: String::new(),
                    plain: String::new(),
                    line: line_of(range.start),
                    end_line: line_of(range.end.saturating_sub(1)),
                });
            }
            Event::End(TagEnd::Heading(_)) => open = false,
            Event::Text(t) if open => {
                if let Some(h) = out.last_mut() {
                    h.label.push_str(&t.replace('[', "\\[").replace(']', "\\]"));
                    h.plain.push_str(&t);
                }
            }
            Event::Code(t) if open => {
                if let Some(h) = out.last_mut() {
                    h.label.push_str(&format!("`{t}`"));
                    h.plain.push_str(&t);
                }
            }
            _ => {}
        }
    }
    out
}

/// Where the front matter at the very start of a file ends: YAML between
/// `---` lines (closed by `---` or `...`), or TOML between `+++` lines, as
/// Jekyll, Hugo, Docusaurus, MkDocs and pandoc write it.
fn front_matter_end(md: &str) -> Option<usize> {
    let bom = if md.starts_with('\u{feff}') { 3 } else { 0 };
    let mut lines = md[bom..].split_inclusive('\n');
    let first = lines.next()?.trim_end();
    let closers: &[&str] = match first {
        "---" => &["---", "..."],
        "+++" => &["+++"],
        _ => return None,
    };
    let sign = if first == "+++" { '=' } else { ':' };
    let mut at = bom + first.len() + 1;
    for line in lines {
        at += line.len();
        if closers.contains(&line.trim_end()) {
            return Some(at.min(md.len()));
        }
        // A rule that opens the text is not front matter: its lines are
        // keys and values, lists, comments or indented continuations.
        let data = line.trim().is_empty()
            || line.starts_with([' ', '\t', '#', '-', '['])
            || line.contains(sign);
        if !data {
            return None;
        }
    }
    None
}

/// What a file's front matter says about the document.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrontMatter {
    pub title: Option<String>,
    pub author: Option<String>,
    pub lang: Option<String>,
}

/// The front matter of a file and the Markdown after it. Front matter is
/// data for a site generator, never text to show.
pub fn split_front_matter(md: &str) -> (FrontMatter, &str) {
    let Some(end) = front_matter_end(md) else {
        return (FrontMatter::default(), md);
    };
    let mut meta = FrontMatter::default();
    for line in md[..end].lines().skip(1) {
        // Top-level keys only: `key: value` in YAML, `key = value` in TOML.
        if line.starts_with([' ', '\t']) {
            continue;
        }
        let Some((key, value)) = line.split_once(':').or_else(|| line.split_once('=')) else {
            continue;
        };
        let value = value.trim();
        let value = value
            .strip_prefix('"')
            .and_then(|v| v.strip_suffix('"'))
            .or_else(|| value.strip_prefix('\'').and_then(|v| v.strip_suffix('\'')))
            .unwrap_or(value)
            .trim();
        if value.is_empty() || value.starts_with(['[', '{', '|', '>']) {
            continue;
        }
        let slot = match key.trim().to_ascii_lowercase().as_str() {
            "title" => &mut meta.title,
            "author" => &mut meta.author,
            "lang" | "language" => &mut meta.lang,
            _ => continue,
        };
        if slot.is_none() {
            *slot = Some(value.to_string());
        }
    }
    (meta, &md[end..])
}

/// The text with the front matter at its very start blanked out, line for
/// line, so line numbers stay the same.
pub fn without_front_matter(md: &str) -> String {
    match front_matter_end(md) {
        Some(end) => {
            let head: String = md[..end].chars().filter(|c| *c == '\n').collect();
            head + &md[end..]
        }
        None => md.to_string(),
    }
}

/// The pictures of raw HTML, as (src, alt).
fn html_pictures(html: &str) -> Vec<(String, String)> {
    use deft_core::text::{attribute, img_tags, unescape_attribute};
    img_tags(html)
        .into_iter()
        .filter_map(|tag| {
            let tag = &html[tag];
            let value = |name: &str| attribute(tag, name).map(|r| unescape_attribute(&tag[r]));
            Some((value("src")?, value("alt").unwrap_or_default()))
        })
        .collect()
}

/// Parse Markdown into blocks. Image paths are kept as written in `Span::image`.
pub fn parse(md: &str) -> Vec<Block> {
    let mut stack = vec![Frame::Top(vec![])];
    let mut spans: Vec<Span> = Vec::new();
    let mut style = Style::default();
    let mut code: Option<String> = None;
    let mut heading: Option<u8> = None;
    let mut table: Option<Vec<Vec<Vec<Span>>>> = None;
    let mut image: Option<(String, String)> = None;

    fn push(stack: &mut [Frame], b: Block) {
        match stack.last_mut().expect("top frame") {
            Frame::Top(v) | Frame::Quote(v) | Frame::Item(v) | Frame::Note(_, v) => v.push(b),
            Frame::List(..) => {}
        }
    }
    fn flush(stack: &mut [Frame], spans: &mut Vec<Span>) {
        if !spans.is_empty() {
            push(stack, Block::Paragraph(std::mem::take(spans)));
        }
    }

    let options = Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES;
    for event in Parser::new_ext(md, options) {
        match event {
            Event::Start(Tag::Heading { level, .. }) => {
                heading = Some(match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    HeadingLevel::H4 => 4,
                    HeadingLevel::H5 => 5,
                    HeadingLevel::H6 => 6,
                });
            }
            Event::End(TagEnd::Heading(_)) => {
                let level = heading.take().unwrap_or(1);
                push(
                    &mut stack,
                    Block::Heading(level, std::mem::take(&mut spans)),
                );
            }
            Event::Start(Tag::Paragraph) => {}
            Event::End(TagEnd::Paragraph) => flush(&mut stack, &mut spans),
            Event::Start(Tag::BlockQuote(_)) => stack.push(Frame::Quote(vec![])),
            Event::End(TagEnd::BlockQuote(_)) => {
                if let Some(Frame::Quote(v)) = stack.pop() {
                    push(&mut stack, Block::Quote(v));
                }
            }
            Event::Start(Tag::List(first)) => {
                // A tight item's text has no paragraph event to flush it.
                flush(&mut stack, &mut spans);
                stack.push(Frame::List(first, vec![]));
            }
            Event::End(TagEnd::List(_)) => {
                if let Some(Frame::List(first, items)) = stack.pop() {
                    push(
                        &mut stack,
                        Block::List {
                            ordered: first.is_some(),
                            start: first.unwrap_or(1),
                            items,
                        },
                    );
                }
            }
            Event::Start(Tag::Item) => stack.push(Frame::Item(vec![])),
            Event::End(TagEnd::Item) => {
                flush(&mut stack, &mut spans);
                if let Some(Frame::Item(v)) = stack.pop()
                    && let Some(Frame::List(_, items)) = stack.last_mut()
                {
                    items.push(v);
                }
            }
            Event::Start(Tag::FootnoteDefinition(label)) => {
                stack.push(Frame::Note(label.to_string(), vec![]));
            }
            Event::End(TagEnd::FootnoteDefinition) => {
                flush(&mut stack, &mut spans);
                if let Some(Frame::Note(label, mut inner)) = stack.pop() {
                    let marker = [Span::note(format!("[{label}]:")), Span::plain(" ")];
                    match inner.first_mut() {
                        Some(Block::Paragraph(first)) => {
                            first.splice(0..0, marker);
                        }
                        _ => inner.insert(0, Block::Paragraph(marker.to_vec())),
                    }
                    for b in inner {
                        push(&mut stack, b);
                    }
                }
            }
            Event::FootnoteReference(label) => spans.push(Span::note(format!("[{label}]"))),
            Event::Start(Tag::CodeBlock(_)) => code = Some(String::new()),
            Event::End(TagEnd::CodeBlock) => {
                let text = code.take().unwrap_or_default();
                push(&mut stack, Block::Code(text.trim_end().to_string()));
            }
            Event::Start(Tag::Table(_)) => table = Some(vec![]),
            Event::End(TagEnd::Table) => {
                if let Some(rows) = table.take() {
                    push(&mut stack, Block::Table(rows, vec![]));
                }
            }
            Event::Start(Tag::TableHead | Tag::TableRow) => {
                if let Some(t) = table.as_mut() {
                    t.push(vec![]);
                }
            }
            Event::End(TagEnd::TableCell) => {
                if let Some(row) = table.as_mut().and_then(|t| t.last_mut()) {
                    row.push(std::mem::take(&mut spans));
                }
            }
            Event::Start(Tag::Emphasis) => style.italic += 1,
            Event::End(TagEnd::Emphasis) => style.italic = style.italic.saturating_sub(1),
            Event::Start(Tag::Strikethrough) => style.strike += 1,
            Event::End(TagEnd::Strikethrough) => style.strike = style.strike.saturating_sub(1),
            Event::Start(Tag::Strong) => style.bold += 1,
            Event::End(TagEnd::Strong) => style.bold = style.bold.saturating_sub(1),
            Event::Start(Tag::Link { dest_url, .. }) => style.link = Some(dest_url.to_string()),
            Event::End(TagEnd::Link) => style.link = None,
            Event::Start(Tag::Image { dest_url, .. }) => {
                image = Some((dest_url.to_string(), String::new()));
            }
            Event::End(TagEnd::Image) => {
                if let Some((url, alt)) = image.take() {
                    spans.push(Span {
                        text: alt,
                        image: Some(url),
                        ..Default::default()
                    });
                }
            }
            Event::Text(t) => {
                if let Some(c) = code.as_mut() {
                    c.push_str(&t);
                } else if let Some((_, alt)) = image.as_mut() {
                    alt.push_str(&t);
                } else {
                    spans.push(span(&t, &style, false));
                }
            }
            Event::Code(t) => spans.push(span(&t, &style, true)),
            Event::SoftBreak => spans.push(span(" ", &style, false)),
            Event::HardBreak => spans.push(span("\n", &style, false)),
            Event::Rule => push(&mut stack, Block::Rule),
            // Pictures written as HTML, as mdBook and READMEs do, are pictures.
            Event::Html(h) | Event::InlineHtml(h) => {
                let compact: String = h.chars().filter(|c| !c.is_whitespace()).collect();
                if matches!(compact.to_ascii_lowercase().as_str(), "<br>" | "<br/>") {
                    spans.push(span("\n", &style, false));
                }
                for (src, alt) in html_pictures(&h) {
                    spans.push(Span {
                        text: alt,
                        image: Some(src),
                        link: style.link.clone(),
                        ..Default::default()
                    });
                }
            }
            Event::End(TagEnd::HtmlBlock) => flush(&mut stack, &mut spans),
            _ => {}
        }
    }
    flush(&mut stack, &mut spans);
    match stack.into_iter().next() {
        Some(Frame::Top(v)) => v,
        _ => vec![],
    }
}
