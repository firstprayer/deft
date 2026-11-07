//! OpenDocument text (ODT): text structure through the same block model.

use crate::zipped::Package;
use crate::{Block, Document, Image, Span, bad};
use deft_core::error::{ErrorCode, Result, err};
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

/// List style name -> numbered, from the first level of the style.
fn list_styles(docs: &[&Xml]) -> HashMap<String, bool> {
    let mut out = HashMap::new();
    for doc in docs {
        for s in doc
            .descendants()
            .filter(|n| n.tag_name().name() == "list-style")
        {
            let Some(name) = s.attribute((STYLE, "name")) else {
                continue;
            };
            let first = s.children().find(Node::is_element);
            let numbered = first.is_some_and(|l| l.tag_name().name() == "list-level-style-number");
            out.insert(name.to_string(), numbered);
        }
    }
    out
}

/// Elements whose content is not part of the text: comments, deleted text
/// kept for change tracking, and the templates of generated indexes.
fn skipped(node: Node) -> bool {
    let name = node.tag_name().name();
    let svg = node
        .tag_name()
        .namespace()
        .is_some_and(|n| n.contains("svg"));
    (svg && matches!(name, "title" | "desc"))
        || matches!(
            name,
            "annotation"
                | "annotation-end"
                | "tracked-changes"
                | "sequence-decls"
                | "variable-decls"
                | "user-field-decls"
                | "forms"
                | "note-citation"
        )
        || name.ends_with("-source")
}

struct Reader<'a> {
    pkg: &'a Package,
    styles: HashMap<String, (bool, bool)>,
    lists: HashMap<String, bool>,
    images: Vec<Image>,
    /// Footnote and endnote bodies, in the order they are referred to.
    notes: Vec<Vec<Span>>,
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
            let name = c.tag_name().name();
            if skipped(c) {
                continue;
            }
            match name {
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
                "s" => {
                    let n = c
                        .attribute((TEXT, "c"))
                        .and_then(|n| n.parse().ok())
                        .unwrap_or(1usize);
                    out.push(Span::plain(" ".repeat(n.min(100))));
                }
                "tab" => out.push(Span::plain("\t")),
                "line-break" => out.push(Span::plain("\n")),
                "note" => {
                    let mut body = Vec::new();
                    if let Some(b) = c.children().find(|x| x.tag_name().name() == "note-body") {
                        self.flat(b, &mut body);
                    }
                    self.notes.push(body);
                    out.push(Span::note(format!("[{}]", self.notes.len())));
                }
                "image" => self.image(c, link, out),
                "frame" => {
                    for d in c.children().filter(Node::is_element) {
                        if d.tag_name().name() == "image" {
                            self.image(d, link, out);
                            // The first picture is the one shown; others are fallbacks.
                            break;
                        }
                    }
                    for d in c.children().filter(|d| d.tag_name().name() == "text-box") {
                        self.spans(d, bold, italic, link, out);
                    }
                }
                // Paragraphs inside shapes and text boxes.
                "p" | "h" => {
                    if out.last().is_some_and(|s| !s.text.ends_with('\n')) {
                        out.push(Span::plain("\n"));
                    }
                    self.spans(c, bold, italic, link, out);
                    out.push(Span::plain("\n"));
                }
                // Fields, references, shapes: their content is what shows.
                _ => self.spans(c, bold, italic, link, out),
            }
        }
    }

    /// All the text under a node on one line, for a note.
    fn flat(&mut self, node: Node, out: &mut Vec<Span>) {
        let mut spans = Vec::new();
        self.spans(node, false, false, None, &mut spans);
        for s in &mut spans {
            s.text = s.text.replace('\n', " ");
        }
        crate::trim_spans(&mut spans);
        out.extend(spans);
    }

    fn image(&mut self, node: Node, link: Option<&str>, out: &mut Vec<Span>) {
        let Some(path) = node.attribute((XLINK, "href")) else {
            return;
        };
        let path = path.trim_start_matches("./");
        let Some(bytes) = self.pkg.read(path) else {
            return;
        };
        let name = path.rsplit('/').next().unwrap_or(path).to_string();
        if !self.images.iter().any(|i| i.name == name) {
            self.images.push(Image {
                name: name.clone(),
                bytes,
            });
        }
        // The frame's title is what a reader would say for the picture.
        let alt = node
            .parent()
            .into_iter()
            .flat_map(|f| f.children())
            .find(|t| t.tag_name().name() == "title")
            .and_then(|t| t.text())
            .unwrap_or("")
            .to_string();
        out.push(Span {
            text: alt,
            image: Some(name),
            link: link.map(String::from),
            ..Default::default()
        });
    }

    fn paragraph(&mut self, node: Node) -> Vec<Span> {
        let mut spans = Vec::new();
        self.spans(node, false, false, None, &mut spans);
        crate::trim_spans(&mut spans);
        spans
    }

    fn blocks(&mut self, parent: Node, out: &mut Vec<Block>) {
        for n in parent.children().filter(Node::is_element) {
            let name = n.tag_name().name();
            if skipped(n) {
                continue;
            }
            match name {
                "h" => {
                    let level = n
                        .attribute((TEXT, "outline-level"))
                        .and_then(|l| l.parse().ok())
                        .unwrap_or(1u8);
                    let spans = self.paragraph(n);
                    if !spans.is_empty() {
                        out.push(Block::Heading(level.clamp(1, 6), spans));
                    }
                }
                "p" => {
                    let spans = self.paragraph(n);
                    if !spans.is_empty() {
                        out.push(Block::Paragraph(spans));
                    }
                }
                "list" => {
                    let ordered = n
                        .attribute((TEXT, "style-name"))
                        .and_then(|s| self.lists.get(s).copied())
                        .unwrap_or(false);
                    let mut items = Vec::new();
                    for item in n
                        .children()
                        .filter(|c| matches!(c.tag_name().name(), "list-item" | "list-header"))
                    {
                        let mut inner = Vec::new();
                        self.blocks(item, &mut inner);
                        if !inner.is_empty() {
                            items.push(inner);
                        }
                    }
                    if !items.is_empty() {
                        out.push(Block::List { ordered, items });
                    }
                }
                "table" => {
                    let mut rows = Vec::new();
                    for r in n
                        .descendants()
                        .filter(|c| c.tag_name().name() == "table-row")
                    {
                        let mut row = Vec::new();
                        for cell in r.children().filter(|c| {
                            matches!(c.tag_name().name(), "table-cell" | "covered-table-cell")
                        }) {
                            let mut spans = Vec::new();
                            for p in cell
                                .children()
                                .filter(|c| matches!(c.tag_name().name(), "p" | "h"))
                            {
                                let more = self.paragraph(p);
                                if !spans.is_empty() && !more.is_empty() {
                                    spans.push(Span::plain(" "));
                                }
                                spans.extend(more);
                            }
                            row.push(spans);
                        }
                        rows.push(row);
                    }
                    out.push(Block::Table(rows, vec![]));
                }
                // Sections, generated indexes and page-anchored shapes.
                _ => self.blocks(n, out),
            }
        }
    }
}

fn meta(pkg: &Package, doc: &mut Document) {
    let Some(text) = pkg.text("meta.xml") else {
        return;
    };
    let Ok(xml) = Xml::parse(&text) else {
        return;
    };
    let get = |name: &str| {
        xml.descendants()
            .find(|n| n.tag_name().name() == name)
            .and_then(|n| n.text())
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
    };
    doc.title = get("title");
    doc.author = get("initial-creator").or_else(|| get("creator"));
}

pub fn read(bytes: &[u8]) -> Result<Document> {
    let pkg = Package::open(bytes)?;
    if pkg
        .text("META-INF/manifest.xml")
        .is_some_and(|m| m.contains("encryption-data"))
    {
        return Err(err(
            ErrorCode::InputEncrypted,
            "odt: the document is password-protected; save it without a password first",
        ));
    }
    let xml = pkg
        .text("content.xml")
        .ok_or_else(|| bad("odt", "no content.xml"))?;
    let doc = Xml::parse(&xml).map_err(|e| bad("odt", e))?;
    let styles_xml = pkg.text("styles.xml").unwrap_or_default();
    let styles_doc = Xml::parse(&styles_xml).ok();
    let body = doc
        .descendants()
        .find(|n| {
            n.tag_name().name() == "text"
                && n.tag_name()
                    .namespace()
                    .is_some_and(|x| x.contains("office"))
        })
        .ok_or_else(|| bad("odt", "no text body"))?;
    let mut all = vec![&doc];
    all.extend(styles_doc.as_ref());
    let mut reader = Reader {
        pkg: &pkg,
        styles: text_styles(&doc),
        lists: list_styles(&all),
        images: vec![],
        notes: vec![],
    };
    let mut blocks = Vec::new();
    reader.blocks(body, &mut blocks);
    if !reader.notes.is_empty() {
        blocks.push(Block::Rule);
        for (n, note) in reader.notes.into_iter().enumerate() {
            let mut spans = vec![Span::note(format!("[{}]:", n + 1)), Span::plain(" ")];
            spans.extend(note);
            blocks.push(Block::Paragraph(spans));
        }
    }
    let mut out = Document {
        blocks,
        images: reader.images,
        ..Default::default()
    };
    meta(&pkg, &mut out);
    Ok(out)
}
