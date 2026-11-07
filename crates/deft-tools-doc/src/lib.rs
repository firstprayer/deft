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
    /// Rows of cells; the first row is the header.
    Table(Vec<Vec<Vec<Span>>>),
    Rule,
    PageBreak,
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
            Block::Table(rows) => {
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

pub(crate) fn bad(what: &str, e: impl std::fmt::Display) -> deft_core::error::DeftError {
    err(ErrorCode::InputCorrupt, format!("{what}: {e}"))
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
