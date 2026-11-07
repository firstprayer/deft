//! Blocks to Markdown text, and Markdown text to blocks.

use crate::{Block, Document, Span};
use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};

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

fn inline(spans: &[Span]) -> String {
    let mut out = String::new();
    for s in spans {
        if let Some(name) = &s.image {
            out.push_str(&format!("![{}]({name})", escape(&s.text)));
            continue;
        }
        if s.note {
            // `[1]` is a reference and `[1]:` starts the note.
            out.push_str(&s.text.replacen('[', "[^", 1));
            continue;
        }
        let text = s.text.trim_matches(' ');
        if text.is_empty() {
            out.push_str(&s.text);
            continue;
        }
        let lead = &s.text[..s.text.len() - s.text.trim_start_matches(' ').len()];
        let trail = &s.text[s.text.trim_end_matches(' ').len()..];
        let mut body = if s.code {
            format!("`{text}`")
        } else {
            escape(text).replace('\n', "  \n")
        };
        if s.strike {
            body = format!("~~{body}~~");
        }
        if s.bold && s.italic {
            body = format!("***{body}***");
        } else if s.bold {
            body = format!("**{body}**");
        } else if s.italic {
            body = format!("*{body}*");
        }
        if let Some(link) = &s.link {
            body = format!("[{body}]({link})");
        }
        out.push_str(lead);
        out.push_str(&body);
        out.push_str(trail);
    }
    out
}

fn table(rows: &[Vec<Vec<Span>>], out: &mut String) {
    let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
    if cols == 0 {
        return;
    }
    let cell = |row: &Vec<Vec<Span>>, i: usize| {
        row.get(i)
            .map(|c| inline(c).replace('|', "\\|").replace('\n', " "))
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

/// `in_item` keeps a nested list against its item, which keeps the list tight.
fn write(blocks: &[Block], prefix: &str, in_item: bool, out: &mut String) {
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
                    out.push_str(line);
                    out.push('\n');
                }
            }
            Block::List { ordered, items } => {
                for (n, item) in items.iter().enumerate() {
                    let marker = if *ordered {
                        format!("{}. ", n + 1)
                    } else {
                        "- ".to_string()
                    };
                    let pad = " ".repeat(marker.len());
                    let mut body = String::new();
                    write(item, "", true, &mut body);
                    for (l, line) in body.lines().enumerate() {
                        out.push_str(prefix);
                        out.push_str(if l == 0 { &marker } else { &pad });
                        out.push_str(line);
                        out.push('\n');
                    }
                }
            }
            Block::Quote(inner) => write(inner, &format!("{prefix}> "), false, out),
            Block::Code(code) => {
                out.push_str(&format!("{prefix}```\n"));
                for line in code.lines() {
                    out.push_str(prefix);
                    out.push_str(line);
                    out.push('\n');
                }
                out.push_str(&format!("{prefix}```\n"));
            }
            Block::Table(rows) => table(rows, out),
            Block::Rule => out.push_str("---\n"),
            Block::PageBreak => out.push_str("<!-- page break -->\n"),
        }
    }
}

/// Render a document as Markdown.
pub fn render(doc: &Document) -> String {
    let mut out = String::new();
    write(&doc.blocks, "", false, &mut out);
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
    List(bool, Vec<Vec<Block>>),
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
    let options = Options::ENABLE_TABLES | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS;
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
                stack.push(Frame::List(first.is_some(), vec![]));
            }
            Event::End(TagEnd::List(_)) => {
                if let Some(Frame::List(ordered, items)) = stack.pop() {
                    push(&mut stack, Block::List { ordered, items });
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
                    push(&mut stack, Block::Table(rows));
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
            _ => {}
        }
    }
    flush(&mut stack, &mut spans);
    match stack.into_iter().next() {
        Some(Frame::Top(v)) => v,
        _ => vec![],
    }
}
