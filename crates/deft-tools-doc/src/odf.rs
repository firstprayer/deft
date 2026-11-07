//! OpenDocument text (ODT): text structure through the same block model.

use crate::zipped::Package;
use crate::{Block, Document, Image, Span, bad};
use deft_core::error::Result;
use roxmltree::{Document as Xml, Node};
use std::collections::HashMap;

const TEXT: &str = "urn:oasis:names:tc:opendocument:xmlns:text:1.0";
const STYLE: &str = "urn:oasis:names:tc:opendocument:xmlns:style:1.0";
const FO: &str = "urn:oasis:names:tc:opendocument:xmlns:xsl-fo-compatible:1.0";
const XLINK: &str = "http://www.w3.org/1999/xlink";

/// Automatic style name -> (bold, italic).
fn text_styles(doc: &Xml) -> HashMap<String, (bool, bool)> {
    let mut out = HashMap::new();
    for s in doc.descendants().filter(|n| n.tag_name().name() == "style") {
        let Some(name) = s.attribute((STYLE, "name")) else {
            continue;
        };
        let props = s
            .children()
            .find(|c| c.tag_name().name() == "text-properties");
        let bold = props.and_then(|p| p.attribute((FO, "font-weight"))) == Some("bold");
        let italic = props.and_then(|p| p.attribute((FO, "font-style"))) == Some("italic");
        out.insert(name.to_string(), (bold, italic));
    }
    out
}

struct Reader<'a> {
    pkg: &'a Package,
    styles: HashMap<String, (bool, bool)>,
    images: Vec<Image>,
}

impl Reader<'_> {
    fn spans(
        &mut self,
        node: Node,
        bold: bool,
        italic: bool,
        link: Option<&str>,
        out: &mut Vec<Span>,
    ) {
        for c in node.children() {
            if c.is_text() {
                let text = c.text().unwrap_or("").replace('\n', " ");
                if !text.is_empty() {
                    out.push(Span {
                        text,
                        bold,
                        italic,
                        link: link.map(String::from),
                        ..Default::default()
                    });
                }
                continue;
            }
            match c.tag_name().name() {
                "span" => {
                    let (b, i) = c
                        .attribute((TEXT, "style-name"))
                        .and_then(|s| self.styles.get(s).copied())
                        .unwrap_or((false, false));
                    self.spans(c, bold || b, italic || i, link, out);
                }
                "a" => {
                    let href = c.attribute((XLINK, "href")).map(String::from);
                    self.spans(c, bold, italic, href.as_deref(), out);
                }
                "s" => out.push(Span::plain(" ")),
                "tab" => out.push(Span::plain("\t")),
                "line-break" => out.push(Span::plain("\n")),
                "frame" => {
                    // Text boxes hold ordinary paragraphs.
                    for p in c.descendants().filter(|d| d.tag_name().name() == "p") {
                        self.spans(p, bold, italic, link, out);
                        out.push(Span::plain("\n"));
                    }
                    if let Some(path) = c
                        .descendants()
                        .find(|d| d.tag_name().name() == "image")
                        .and_then(|d| d.attribute((XLINK, "href")))
                        && let Some(bytes) = self.pkg.read(path)
                    {
                        let name = path.rsplit('/').next().unwrap_or(path).to_string();
                        self.images.push(Image {
                            name: name.clone(),
                            bytes,
                        });
                        out.push(Span {
                            image: Some(name),
                            ..Default::default()
                        });
                    }
                }
                _ => {}
            }
        }
    }

    fn paragraph(&mut self, node: Node) -> Vec<Span> {
        let mut spans = Vec::new();
        self.spans(node, false, false, None, &mut spans);
        spans
    }

    fn blocks(&mut self, parent: Node, out: &mut Vec<Block>) {
        for n in parent.children().filter(Node::is_element) {
            match n.tag_name().name() {
                "h" => {
                    let level = n
                        .attribute((TEXT, "outline-level"))
                        .and_then(|l| l.parse().ok())
                        .unwrap_or(1u8);
                    out.push(Block::Heading(level, self.paragraph(n)));
                }
                "p" => {
                    let spans = self.paragraph(n);
                    if !spans.is_empty() {
                        out.push(Block::Paragraph(spans));
                    }
                }
                "list" => {
                    let mut items = Vec::new();
                    for item in n.children().filter(|c| c.tag_name().name() == "list-item") {
                        let mut inner = Vec::new();
                        self.blocks(item, &mut inner);
                        items.push(inner);
                    }
                    out.push(Block::List {
                        ordered: false,
                        items,
                    });
                }
                "table" => {
                    let mut rows = Vec::new();
                    for r in n
                        .descendants()
                        .filter(|c| c.tag_name().name() == "table-row")
                    {
                        let mut row = Vec::new();
                        for cell in r.children().filter(|c| c.tag_name().name() == "table-cell") {
                            let mut spans = Vec::new();
                            for p in cell.children().filter(|c| c.tag_name().name() == "p") {
                                spans.extend(self.paragraph(p));
                            }
                            row.push(spans);
                        }
                        rows.push(row);
                    }
                    out.push(Block::Table(rows));
                }
                "section" => self.blocks(n, out),
                _ => {}
            }
        }
    }
}

pub fn read(bytes: &[u8]) -> Result<Document> {
    let pkg = Package::open(bytes)?;
    let xml = pkg
        .text("content.xml")
        .ok_or_else(|| bad("odt", "no content.xml"))?;
    let doc = Xml::parse(&xml).map_err(|e| bad("odt", e))?;
    let body = doc
        .descendants()
        .find(|n| {
            n.tag_name().name() == "text"
                && n.tag_name()
                    .namespace()
                    .is_some_and(|x| x.contains("office"))
        })
        .ok_or_else(|| bad("odt", "no text body"))?;
    let mut reader = Reader {
        pkg: &pkg,
        styles: text_styles(&doc),
        images: vec![],
    };
    let mut blocks = Vec::new();
    reader.blocks(body, &mut blocks);
    Ok(Document {
        blocks,
        images: reader.images,
        ..Default::default()
    })
}
