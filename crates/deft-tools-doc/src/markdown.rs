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

fn write(blocks: &[Block], prefix: &str, out: &mut String) {
    for (i, b) in blocks.iter().enumerate() {
        if i > 0 {
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
                    write(item, "", &mut body);
                    for (l, line) in body.lines().enumerate() {
                        out.push_str(prefix);
                        out.push_str(if l == 0 { &marker } else { &pad });
                        out.push_str(line);
                        out.push('\n');
                    }
                }
            }
            Block::Quote(inner) => write(inner, &format!("{prefix}> "), out),
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
    write(&doc.blocks, "", &mut out);
    out
}

#[derive(Default)]
struct Style {
    bold: u32,
    italic: u32,
    link: Option<String>,
}

fn span(text: &str, style: &Style, code: bool) -> Span {
    Span {
        text: text.to_string(),
        bold: style.bold > 0,
        italic: style.italic > 0,
        code,
        link: style.link.clone(),
        image: None,
    }
}

enum Frame {
    Top(Vec<Block>),
    Quote(Vec<Block>),
    List(bool, Vec<Vec<Block>>),
    Item(Vec<Block>),
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
            Frame::Top(v) | Frame::Quote(v) | Frame::Item(v) => v.push(b),
            Frame::List(..) => {}
        }
    }
    fn flush(stack: &mut [Frame], spans: &mut Vec<Span>) {
        if !spans.is_empty() {
            push(stack, Block::Paragraph(std::mem::take(spans)));
        }
    }

    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
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
