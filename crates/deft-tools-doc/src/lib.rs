//! Office and web documents through one block model: read Word, ODT, RTF,
//! PowerPoint, HTML and EPUB; write Markdown, plain text, Word and EPUB.

use deft_core::error::{ErrorCode, err};

pub mod docx;
mod dom;
pub mod epub;
pub mod html;
pub mod markdown;
pub mod odf;
pub mod pptx;
pub mod rtf;
pub mod xhtml;
mod zipped;

/// Inline text with its style.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
    pub strike: bool,
    pub link: Option<String>,
    /// Name of an image in `Document::images`, instead of text.
    pub image: Option<String>,
    /// A note marker: `[1]` in the text, `[1]:` where the note itself starts.
    pub note: bool,
}

impl Span {
    pub fn plain(text: impl Into<String>) -> Span {
        Span {
            text: text.into(),
            ..Default::default()
        }
    }

    /// A note marker, see `Span::note`.
    pub fn note(text: impl Into<String>) -> Span {
        Span {
            text: text.into(),
            note: true,
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Heading(u8, Vec<Span>),
    Paragraph(Vec<Span>),
    /// Items, each a list of blocks.
    List {
        ordered: bool,
        items: Vec<Vec<Block>>,
    },
    Quote(Vec<Block>),
    Code(String),
    /// Rows of cells; the first row is the header. Every row has a cell for
    /// each grid column; merged cells are listed beside the rows.
    Table(Vec<Vec<Vec<Span>>>, Vec<Merge>),
    Rule,
    PageBreak,
}

/// A merged cell: the cell at `row`, `col` covers `rows` by `cols` cells of
/// the grid. The cells it covers are empty.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Merge {
    pub row: usize,
    pub col: usize,
    pub rows: usize,
    pub cols: usize,
}

/// An embedded picture.
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub name: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Document {
    pub title: Option<String>,
    pub author: Option<String>,
    pub blocks: Vec<Block>,
    pub images: Vec<Image>,
    /// What the reader could not keep, for the user to hear about.
    pub warnings: Vec<String>,
    /// PNG versions of SVG pictures, by picture name, drawn at twice their
    /// size, for writers whose readers need one.
    pub fallbacks: std::collections::HashMap<String, Vec<u8>>,
}

fn spans_text(spans: &[Span], out: &mut String) {
    for s in spans {
        out.push_str(&s.text);
    }
}

fn blocks_text(blocks: &[Block], out: &mut String) {
    for b in blocks {
        match b {
            Block::Heading(_, s) | Block::Paragraph(s) => {
                spans_text(s, out);
                out.push('\n');
            }
            Block::List { items, .. } => {
                for item in items {
                    blocks_text(item, out);
                }
            }
            Block::Quote(inner) => blocks_text(inner, out),
            Block::Code(c) => {
                out.push_str(c);
                out.push('\n');
            }
            Block::Table(rows, _) => {
                for row in rows {
                    let cells: Vec<String> = row
                        .iter()
                        .map(|c| {
                            let mut t = String::new();
                            spans_text(c, &mut t);
                            t
                        })
                        .collect();
                    out.push_str(&cells.join("\t"));
                    out.push('\n');
                }
            }
            Block::Rule => {}
            Block::PageBreak => out.push('\x0c'),
        }
        out.push('\n');
    }
}

impl Document {
    /// Plain text, one paragraph per line block.
    pub fn text(&self) -> String {
        let mut out = String::new();
        blocks_text(&self.blocks, &mut out);
        let trimmed = out.trim_end().to_string();
        trimmed + "\n"
    }

    pub fn words(&self) -> usize {
        self.text().split_whitespace().count()
    }
}

/// Drop blank spans and spaces at both ends of a line of spans.
pub(crate) fn trim_spans(spans: &mut Vec<Span>) {
    while spans
        .last()
        .is_some_and(|s| s.image.is_none() && s.text.trim().is_empty())
    {
        spans.pop();
    }
    if let Some(last) = spans.last_mut()
        && last.image.is_none()
    {
        last.text = last.text.trim_end().to_string();
    }
    while spans
        .first()
        .is_some_and(|s| s.image.is_none() && s.text.trim().is_empty())
    {
        spans.remove(0);
    }
    if let Some(first) = spans.first_mut()
        && first.image.is_none()
        && !first.note
    {
        first.text = first.text.trim_start().to_string();
    }
}

pub(crate) fn bad(what: &str, e: impl std::fmt::Display) -> deft_core::error::DeftError {
    err(ErrorCode::InputCorrupt, format!("{what}: {e}"))
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
