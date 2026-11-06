//! Word documents: a reader for the common structures and a small writer.

use crate::zipped::{Package, join};
use crate::{Block, Document, Image, Span, bad};
use deft_core::error::Result;
use roxmltree::{Document as Xml, Node};
use std::collections::HashMap;
use std::io::Write;

const W: &str = "http://schemas.openxmlformats.org/wordprocessingml/2006/main";
const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const A: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";

fn w<'a, 'i>(node: Node<'a, 'i>, name: &str) -> Option<Node<'a, 'i>> {
    node.children().find(|c| {
        c.is_element() && c.tag_name().name() == name && c.tag_name().namespace() == Some(W)
    })
}

fn flag(rpr: Option<Node>, name: &str) -> bool {
    rpr.and_then(|p| w(p, name))
        .is_some_and(|n| !matches!(n.attribute((W, "val")), Some("0" | "false" | "none")))
}

/// Relationship id -> target, for one part.
fn relationships(pkg: &Package, part: &str) -> HashMap<String, (String, bool)> {
    let (dir, file) = part.rsplit_once('/').unwrap_or(("", part));
    let path = format!("{dir}/_rels/{file}.rels");
    let mut out = HashMap::new();
    if let Some(text) = pkg.text(&path)
        && let Ok(doc) = Xml::parse(&text)
    {
        for r in doc.root_element().children().filter(Node::is_element) {
            if let (Some(id), Some(target)) = (r.attribute("Id"), r.attribute("Target")) {
                let external = r.attribute("TargetMode") == Some("External");
                out.insert(id.to_string(), (target.to_string(), external));
            }
        }
    }
    out
}

struct Reader<'a> {
    pkg: &'a Package,
    rels: HashMap<String, (String, bool)>,
    styles: HashMap<String, u8>,
    /// numId -> ordered, from numbering.xml.
    ordered: HashMap<String, bool>,
    images: Vec<Image>,
    seen: HashMap<String, String>,
}

impl Reader<'_> {
    fn image(&mut self, rid: &str) -> Option<String> {
        let (target, _) = self.rels.get(rid)?.clone();
        let path = join("word/document.xml", &target);
        if let Some(name) = self.seen.get(&path) {
            return Some(name.clone());
        }
        let bytes = self.pkg.read(&path)?;
        let name = path.rsplit('/').next()?.to_string();
        self.images.push(Image {
            name: name.clone(),
            bytes,
        });
        self.seen.insert(path, name.clone());
        Some(name)
    }

    fn run(&mut self, r: Node, link: Option<&str>, out: &mut Vec<Span>) {
        let rpr = w(r, "rPr");
        let (bold, italic) = (flag(rpr, "b"), flag(rpr, "i"));
        let code = rpr
            .and_then(|p| w(p, "rFonts"))
            .and_then(|f| f.attribute((W, "ascii")))
            .is_some_and(|f| f.contains("Courier") || f.contains("Consolas") || f.contains("Mono"));
        let mut text = String::new();
        for c in r.children().filter(Node::is_element) {
            match c.tag_name().name() {
                "t" => text.push_str(c.text().unwrap_or("")),
                "tab" => text.push('\t'),
                "br" if c.attribute((W, "type")) != Some("page") => text.push('\n'),
                "drawing" | "pict" => {
                    let rid = c
                        .descendants()
                        .find(|d| {
                            d.tag_name().name() == "blip" && d.tag_name().namespace() == Some(A)
                        })
                        .and_then(|b| b.attribute((R, "embed")));
                    if let Some(name) = rid.and_then(|id| self.image(id)) {
                        out.push(Span {
                            image: Some(name),
                            ..Default::default()
                        });
                    }
                }
                _ => {}
            }
        }
        if !text.is_empty() {
            out.push(Span {
                text,
                bold,
                italic,
                code,
                link: link.map(String::from),
                image: None,
            });
        }
    }

    fn spans(&mut self, p: Node) -> Vec<Span> {
        let mut out: Vec<Span> = Vec::new();
        for c in p.children().filter(Node::is_element) {
            match c.tag_name().name() {
                "r" => self.run(c, None, &mut out),
                "hyperlink" => {
                    let target = c
                        .attribute((R, "id"))
                        .and_then(|id| self.rels.get(id))
                        .map(|(t, _)| t.clone());
                    for r in c.children().filter(|x| x.tag_name().name() == "r") {
                        self.run(r, target.as_deref(), &mut out);
                    }
                }
                "ins" | "smartTag" | "sdt" | "sdtContent" | "fldSimple" => {
                    for r in c.descendants().filter(|x| x.tag_name().name() == "r") {
                        self.run(r, None, &mut out);
                    }
                }
                _ => {}
            }
        }
        out
    }

    fn table(&mut self, t: Node) -> Block {
        let mut rows = Vec::new();
        for tr in t.children().filter(|c| c.tag_name().name() == "tr") {
            let mut row = Vec::new();
            for tc in tr.children().filter(|c| c.tag_name().name() == "tc") {
                let mut cell = Vec::new();
                for p in tc.children().filter(|c| c.tag_name().name() == "p") {
                    if !cell.is_empty() {
                        cell.push(Span::plain(" "));
                    }
                    cell.extend(self.spans(p));
                }
                row.push(cell);
            }
            rows.push(row);
        }
        Block::Table(rows)
    }

    fn paragraph(&mut self, p: Node, blocks: &mut Vec<Block>) {
        let ppr = w(p, "pPr");
        let style = ppr
            .and_then(|x| w(x, "pStyle"))
            .and_then(|s| s.attribute((W, "val")))
            .unwrap_or("");
        let num = ppr.and_then(|x| w(x, "numPr"));
        let breaks_page = p
            .descendants()
            .any(|d| d.tag_name().name() == "br" && d.attribute((W, "type")) == Some("page"));
        let spans = self.spans(p);
        if breaks_page {
            blocks.push(Block::PageBreak);
        }
        if spans.is_empty() {
            return;
        }
        if let Some(level) = self.styles.get(style) {
            if *level == 0 {
                blocks.push(Block::Heading(1, spans));
            } else {
                blocks.push(Block::Heading(*level, spans));
            }
            return;
        }
        if let Some(n) = num {
            let id = w(n, "numId")
                .and_then(|x| x.attribute((W, "val")))
                .unwrap_or("");
            let ordered = self.ordered.get(id).copied().unwrap_or(false);
            let item = vec![Block::Paragraph(spans)];
            if let Some(Block::List { ordered: o, items }) = blocks.last_mut()
                && *o == ordered
            {
                items.push(item);
                return;
            }
            blocks.push(Block::List {
                ordered,
                items: vec![item],
            });
            return;
        }
        let lower = style.to_lowercase();
        if lower.contains("quote") {
            blocks.push(Block::Quote(vec![Block::Paragraph(spans)]));
        } else if lower.contains("code") || lower.contains("sourcecode") {
            let text: String = spans.iter().map(|s| s.text.as_str()).collect();
            blocks.push(Block::Code(text));
        } else {
            blocks.push(Block::Paragraph(spans));
        }
    }
}

/// Style id -> heading level, from styles.xml.
fn heading_styles(pkg: &Package) -> HashMap<String, u8> {
    let mut out = HashMap::new();
    for n in 1..=9u8 {
        out.insert(format!("Heading{n}"), n);
        out.insert(format!("heading{n}"), n);
    }
    out.insert("Title".into(), 1);
    let Some(text) = pkg.text("word/styles.xml") else {
        return out;
    };
    let Ok(doc) = Xml::parse(&text) else {
        return out;
    };
    for s in doc.descendants().filter(|n| n.tag_name().name() == "style") {
        let (Some(id), Some(name)) = (
            s.attribute((W, "styleId")),
            w(s, "name").and_then(|n| n.attribute((W, "val"))),
        ) else {
            continue;
        };
        let lower = name.to_lowercase();
        if let Some(n) = lower
            .strip_prefix("heading ")
            .and_then(|n| n.trim().parse().ok())
        {
            out.insert(id.to_string(), n);
        } else if lower == "title" {
            out.insert(id.to_string(), 1);
        }
    }
    out
}

/// numId -> ordered, following numId -> abstractNum -> level 0 format.
fn numbering(pkg: &Package) -> HashMap<String, bool> {
    let mut out = HashMap::new();
    let Some(text) = pkg.text("word/numbering.xml") else {
        return out;
    };
    let Ok(doc) = Xml::parse(&text) else {
        return out;
    };
    let mut abstract_ordered = HashMap::new();
    for a in doc
        .descendants()
        .filter(|n| n.tag_name().name() == "abstractNum")
    {
        let id = a.attribute((W, "abstractNumId")).unwrap_or("");
        let format = a
            .children()
            .find(|c| c.tag_name().name() == "lvl")
            .and_then(|l| w(l, "numFmt"))
            .and_then(|f| f.attribute((W, "val")))
            .unwrap_or("bullet");
        abstract_ordered.insert(id.to_string(), format != "bullet");
    }
    for n in doc.descendants().filter(|n| n.tag_name().name() == "num") {
        let id = n.attribute((W, "numId")).unwrap_or("");
        let abs = w(n, "abstractNumId")
            .and_then(|x| x.attribute((W, "val")))
            .unwrap_or("");
        out.insert(
            id.to_string(),
            abstract_ordered.get(abs).copied().unwrap_or(false),
        );
    }
    out
}

fn core_properties(pkg: &Package, doc: &mut Document) {
    let Some(text) = pkg.text("docProps/core.xml") else {
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
    doc.author = get("creator");
}

pub fn read(bytes: &[u8]) -> Result<Document> {
    let pkg = Package::open(bytes)?;
    let xml = pkg
        .text("word/document.xml")
        .ok_or_else(|| bad("docx", "no word/document.xml"))?;
    let doc = Xml::parse(&xml).map_err(|e| bad("docx", e))?;
    let body = w(doc.root_element(), "body").ok_or_else(|| bad("docx", "no document body"))?;
    let mut reader = Reader {
        pkg: &pkg,
        rels: relationships(&pkg, "word/document.xml"),
        styles: heading_styles(&pkg),
        ordered: numbering(&pkg),
        images: vec![],
        seen: HashMap::new(),
    };
    let mut blocks = Vec::new();
    for node in body.children().filter(Node::is_element) {
        match node.tag_name().name() {
            "p" => reader.paragraph(node, &mut blocks),
            "tbl" => {
                let t = reader.table(node);
                blocks.push(t);
            }
            "sdt" => {
                if let Some(content) = w(node, "sdtContent") {
                    for p in content.children().filter(|c| c.tag_name().name() == "p") {
                        reader.paragraph(p, &mut blocks);
                    }
                }
            }
            _ => {}
        }
    }
    let mut out = Document {
        blocks,
        images: reader.images,
        ..Default::default()
    };
    core_properties(&pkg, &mut out);
    Ok(out)
}

/// Replace text in a document, keeping everything else. Matching works across
/// runs inside a paragraph, because Word splits text at arbitrary places.
/// Returns the new file and how many replacements were made.
pub fn replace(bytes: &[u8], find: &str, with: &str) -> Result<(Vec<u8>, usize)> {
    let pkg = Package::open(bytes)?;
    let mut total = 0;
    let mut changed = HashMap::new();
    for name in pkg.names() {
        let is_text_part = name == "word/document.xml"
            || (name.starts_with("word/header") && name.ends_with(".xml"))
            || (name.starts_with("word/footer") && name.ends_with(".xml"));
        if !is_text_part {
            continue;
        }
        let xml = pkg.text(&name).ok_or_else(|| bad("docx", &name))?;
        let (new, n) = replace_part(&xml, find, with)?;
        total += n;
        if n > 0 {
            changed.insert(name, new);
        }
    }
    let mut zip_out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for name in pkg.names() {
        let data = match changed.get(&name) {
            Some(text) => text.clone().into_bytes(),
            None => pkg.read(&name).ok_or_else(|| bad("docx", &name))?,
        };
        zip_out
            .start_file(name, options)
            .map_err(|e| bad("docx", e))?;
        zip_out.write_all(&data).map_err(|e| bad("docx", e))?;
    }
    let out = zip_out.finish().map_err(|e| bad("docx", e))?.into_inner();
    Ok((out, total))
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn unescape(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// Find `<w:t ...>text</w:t>` spans paragraph by paragraph; a match spanning
/// runs lands in the first of them and is removed from the others.
fn replace_part(xml: &str, find: &str, with: &str) -> Result<(String, usize)> {
    let t =
        regex::Regex::new(r"(<w:t(?:\s[^>]*)?>)([^<]*)(</w:t>)").map_err(|e| bad("regex", e))?;
    let para = regex::Regex::new(r"(?s)<w:p[ >].*?</w:p>").map_err(|e| bad("regex", e))?;
    let mut total = 0;
    let mut out = String::with_capacity(xml.len());
    let mut last = 0;
    for m in para.find_iter(xml) {
        out.push_str(&xml[last..m.start()]);
        last = m.end();
        let p = m.as_str();
        let pieces: Vec<(usize, usize, String)> = t
            .captures_iter(p)
            .map(|c| {
                let g = c.get(2).expect("group");
                (g.start(), g.end(), unescape(g.as_str()))
            })
            .collect();
        let joined: String = pieces.iter().map(|x| x.2.as_str()).collect();
        let n = joined.matches(find).count();
        if n == 0 {
            out.push_str(p);
            continue;
        }
        total += n;
        // Walk the matches, mapping them back onto the pieces.
        let mut new_text: Vec<String> = pieces.iter().map(|x| x.2.clone()).collect();
        let starts: Vec<usize> = pieces
            .iter()
            .scan(0, |at, x| {
                let s = *at;
                *at += x.2.len();
                Some(s)
            })
            .collect();
        let hits: Vec<usize> = joined.match_indices(find).map(|(i, _)| i).collect();
        for &hit in hits.iter().rev() {
            let end = hit + find.len();
            let mut placed = false;
            for i in (0..pieces.len()).rev() {
                let (s, e) = (starts[i], starts[i] + pieces[i].2.len());
                if e <= hit || s >= end {
                    continue;
                }
                let (from, to) = (hit.max(s) - s, end.min(e) - s);
                let is_first = hit >= s;
                let piece = &mut new_text[i];
                let replacement = if is_first { with } else { "" };
                piece.replace_range(from..to, replacement);
                placed |= is_first;
            }
            debug_assert!(placed);
        }
        let mut rebuilt = String::with_capacity(p.len());
        let mut at = 0;
        for (i, (s, e, _)) in pieces.iter().enumerate() {
            rebuilt.push_str(&p[at..*s]);
            rebuilt.push_str(&escape(&new_text[i]));
            at = *e;
        }
        rebuilt.push_str(&p[at..]);
        // Spaces at the edge of a run vanish without this attribute.
        out.push_str(&rebuilt.replace("<w:t>", "<w:t xml:space=\"preserve\">"));
    }
    out.push_str(&xml[last..]);
    Ok((out, total))
}

// ------------------------------------------------------------------ writer

fn xml_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

struct Writer<'a> {
    doc: &'a Document,
    rels: Vec<String>,
    media: Vec<(String, &'a [u8])>,
    next_id: usize,
    picture: usize,
}

impl Writer<'_> {
    fn rel(&mut self, kind: &str, target: &str, external: bool) -> String {
        self.next_id += 1;
        let id = format!("rId{}", self.next_id);
        self.rels.push(format!(
            "<Relationship Id=\"{id}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}\" Target=\"{}\"{}/>",
            xml_text(target),
            if external { " TargetMode=\"External\"" } else { "" }
        ));
        id
    }

    fn run(&self, s: &Span, extra: &str) -> String {
        let mut props = String::new();
        if s.bold {
            props.push_str("<w:b/>");
        }
        if s.italic {
            props.push_str("<w:i/>");
        }
        if s.code {
            props.push_str(
                "<w:rFonts w:ascii=\"Consolas\" w:hAnsi=\"Consolas\" w:cs=\"Consolas\"/>",
            );
        }
        props.push_str(extra);
        let mut body = String::new();
        for (i, line) in s.text.split('\n').enumerate() {
            if i > 0 {
                body.push_str("<w:br/>");
            }
            body.push_str(&format!(
                "<w:t xml:space=\"preserve\">{}</w:t>",
                xml_text(line)
            ));
        }
        format!("<w:r><w:rPr>{props}</w:rPr>{body}</w:r>")
    }

    fn picture(&mut self, name: &str) -> String {
        let Some(image) = self.doc.images.iter().find(|i| i.name == name) else {
            return String::new();
        };
        let file = name.rsplit('/').next().unwrap_or(name).to_string();
        let target = format!("media/{file}");
        let rid = self.rel("image", &target, false);
        self.media.push((file.clone(), &image.bytes));
        self.picture += 1;
        let (w, h) = image_size(&image.bytes).unwrap_or((640, 480));
        // 96 dpi to EMU, limited to the text width of a 6.5 inch column.
        let scale = (5_943_600.0 / (w as f64 * 9525.0)).min(1.0);
        let (cx, cy) = (
            (w as f64 * 9525.0 * scale) as u64,
            (h as f64 * 9525.0 * scale) as u64,
        );
        let n = self.picture;
        format!(
            "<w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:docPr id=\"{n}\" name=\"Picture {n}\"/><a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:nvPicPr><pic:cNvPr id=\"{n}\" name=\"{}\"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed=\"{rid}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>",
            xml_text(&file)
        )
    }

    fn spans(&mut self, spans: &[Span]) -> String {
        let mut out = String::new();
        for s in spans {
            if let Some(name) = &s.image {
                out.push_str(&self.picture(name));
            } else if let Some(link) = &s.link {
                let rid = self.rel("hyperlink", link, true);
                let run = self.run(s, "<w:color w:val=\"0563C1\"/><w:u w:val=\"single\"/>");
                out.push_str(&format!("<w:hyperlink r:id=\"{rid}\">{run}</w:hyperlink>"));
            } else {
                out.push_str(&self.run(s, ""));
            }
        }
        out
    }

    fn block(&mut self, b: &Block, depth: usize, quote: bool, out: &mut String) {
        let indent = if quote {
            "<w:pStyle w:val=\"Quote\"/>"
        } else {
            ""
        };
        match b {
            Block::Heading(level, spans) => {
                let body = self.spans(spans);
                out.push_str(&format!(
                    "<w:p><w:pPr><w:pStyle w:val=\"Heading{}\"/></w:pPr>{body}</w:p>",
                    (*level).clamp(1, 6)
                ));
            }
            Block::Paragraph(spans) => {
                let body = self.spans(spans);
                out.push_str(&format!("<w:p><w:pPr>{indent}</w:pPr>{body}</w:p>"));
            }
            Block::List { ordered, items } => {
                let id = if *ordered { 2 } else { 1 };
                for item in items {
                    for (i, inner) in item.iter().enumerate() {
                        match inner {
                            Block::Paragraph(spans) if i == 0 => {
                                let body = self.spans(spans);
                                out.push_str(&format!(
                                    "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"{}\"/><w:numId w:val=\"{id}\"/></w:numPr></w:pPr>{body}</w:p>",
                                    depth.min(8)
                                ));
                            }
                            other => self.block(other, depth + 1, quote, out),
                        }
                    }
                }
            }
            Block::Quote(inner) => {
                for x in inner {
                    self.block(x, depth, true, out);
                }
            }
            Block::Code(text) => {
                for line in text.lines() {
                    let run = self.run(
                        &Span {
                            text: line.to_string(),
                            code: true,
                            ..Default::default()
                        },
                        "",
                    );
                    out.push_str(&format!(
                        "<w:p><w:pPr><w:pStyle w:val=\"Code\"/></w:pPr>{run}</w:p>"
                    ));
                }
            }
            Block::Table(rows) => {
                out.push_str("<w:tbl><w:tblPr><w:tblStyle w:val=\"TableGrid\"/><w:tblW w:w=\"5000\" w:type=\"pct\"/><w:tblBorders><w:top w:val=\"single\" w:sz=\"4\" w:color=\"auto\"/><w:left w:val=\"single\" w:sz=\"4\" w:color=\"auto\"/><w:bottom w:val=\"single\" w:sz=\"4\" w:color=\"auto\"/><w:right w:val=\"single\" w:sz=\"4\" w:color=\"auto\"/><w:insideH w:val=\"single\" w:sz=\"4\" w:color=\"auto\"/><w:insideV w:val=\"single\" w:sz=\"4\" w:color=\"auto\"/></w:tblBorders></w:tblPr>");
                for (r, row) in rows.iter().enumerate() {
                    out.push_str("<w:tr>");
                    for cell in row {
                        let mut spans = cell.clone();
                        if r == 0 {
                            spans.iter_mut().for_each(|s| s.bold = true);
                        }
                        let body = self.spans(&spans);
                        out.push_str(&format!("<w:tc><w:tcPr/><w:p>{body}</w:p></w:tc>"));
                    }
                    out.push_str("</w:tr>");
                }
                out.push_str("</w:tbl><w:p/>");
            }
            Block::Rule => out.push_str(
                "<w:p><w:pPr><w:pBdr><w:bottom w:val=\"single\" w:sz=\"6\" w:space=\"1\" w:color=\"auto\"/></w:pBdr></w:pPr></w:p>",
            ),
            Block::PageBreak => out.push_str("<w:p><w:r><w:br w:type=\"page\"/></w:r></w:p>"),
        }
    }
}

pub(crate) fn image_size(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.starts_with(b"\x89PNG") && bytes.len() > 24 {
        return Some((
            u32::from_be_bytes(bytes[16..20].try_into().ok()?),
            u32::from_be_bytes(bytes[20..24].try_into().ok()?),
        ));
    }
    if bytes.starts_with(&[0xFF, 0xD8]) {
        let mut i = 2;
        while i + 9 < bytes.len() {
            if bytes[i] != 0xFF {
                return None;
            }
            let marker = bytes[i + 1];
            let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
            if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
                let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
                let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32;
                return Some((w, h));
            }
            i += 2 + len;
        }
    }
    None
}

const CONTENT_TYPES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"png\" ContentType=\"image/png\"/><Default Extension=\"jpg\" ContentType=\"image/jpeg\"/><Default Extension=\"jpeg\" ContentType=\"image/jpeg\"/><Default Extension=\"gif\" ContentType=\"image/gif\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/><Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/><Override PartName=\"/word/numbering.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml\"/><Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/></Types>";

const ROOT_RELS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"word/document.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties\" Target=\"docProps/core.xml\"/></Relationships>";

fn styles() -> String {
    let mut s = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:styles xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\"><w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii=\"Calibri\" w:hAnsi=\"Calibri\" w:eastAsia=\"Microsoft YaHei\" w:cs=\"Calibri\"/><w:sz w:val=\"22\"/></w:rPr></w:rPrDefault><w:pPrDefault><w:pPr><w:spacing w:after=\"120\" w:line=\"264\" w:lineRule=\"auto\"/></w:pPr></w:pPrDefault></w:docDefaults><w:style w:type=\"paragraph\" w:default=\"1\" w:styleId=\"Normal\"><w:name w:val=\"Normal\"/></w:style>",
    );
    for (n, size) in [(1, 36), (2, 30), (3, 26), (4, 24), (5, 22), (6, 22)] {
        s.push_str(&format!("<w:style w:type=\"paragraph\" w:styleId=\"Heading{n}\"><w:name w:val=\"heading {n}\"/><w:basedOn w:val=\"Normal\"/><w:pPr><w:keepNext/><w:spacing w:before=\"240\" w:after=\"80\"/><w:outlineLvl w:val=\"{}\"/></w:pPr><w:rPr><w:b/><w:sz w:val=\"{size}\"/></w:rPr></w:style>", n - 1));
    }
    s.push_str("<w:style w:type=\"paragraph\" w:styleId=\"Quote\"><w:name w:val=\"Quote\"/><w:basedOn w:val=\"Normal\"/><w:pPr><w:ind w:left=\"480\"/></w:pPr><w:rPr><w:i/></w:rPr></w:style><w:style w:type=\"paragraph\" w:styleId=\"Code\"><w:name w:val=\"Code\"/><w:basedOn w:val=\"Normal\"/><w:pPr><w:spacing w:after=\"0\"/><w:shd w:val=\"clear\" w:fill=\"F2F2F2\"/></w:pPr><w:rPr><w:rFonts w:ascii=\"Consolas\" w:hAnsi=\"Consolas\"/><w:sz w:val=\"20\"/></w:rPr></w:style><w:style w:type=\"table\" w:styleId=\"TableGrid\"><w:name w:val=\"Table Grid\"/><w:tblPr><w:tblBorders><w:top w:val=\"single\" w:sz=\"4\" w:color=\"auto\"/><w:left w:val=\"single\" w:sz=\"4\" w:color=\"auto\"/><w:bottom w:val=\"single\" w:sz=\"4\" w:color=\"auto\"/><w:right w:val=\"single\" w:sz=\"4\" w:color=\"auto\"/><w:insideH w:val=\"single\" w:sz=\"4\" w:color=\"auto\"/><w:insideV w:val=\"single\" w:sz=\"4\" w:color=\"auto\"/></w:tblBorders></w:tblPr></w:style></w:styles>");
    s
}

fn numbering_xml() -> String {
    let mut s = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:numbering xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\">",
    );
    for (id, format, text) in [(0, "bullet", "\u{2022}"), (1, "decimal", "%1.")] {
        s.push_str(&format!("<w:abstractNum w:abstractNumId=\"{id}\">"));
        for level in 0..9 {
            let body = if format == "decimal" {
                format!("%{}.", level + 1)
            } else {
                text.to_string()
            };
            s.push_str(&format!(
                "<w:lvl w:ilvl=\"{level}\"><w:start w:val=\"1\"/><w:numFmt w:val=\"{format}\"/><w:lvlText w:val=\"{body}\"/><w:lvlJc w:val=\"left\"/><w:pPr><w:ind w:left=\"{}\" w:hanging=\"360\"/></w:pPr></w:lvl>",
                720 * (level + 1)
            ));
        }
        s.push_str("</w:abstractNum>");
    }
    s.push_str("<w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num><w:num w:numId=\"2\"><w:abstractNumId w:val=\"1\"/></w:num></w:numbering>");
    s
}

/// Write a Word document.
pub fn write(doc: &Document) -> Result<Vec<u8>> {
    let mut writer = Writer {
        doc,
        rels: vec![
            "<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>".to_string(),
            "<Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering\" Target=\"numbering.xml\"/>".to_string(),
        ],
        media: vec![],
        next_id: 2,
        picture: 0,
    };
    let mut body = String::new();
    for b in &doc.blocks {
        writer.block(b, 0, false, &mut body);
    }
    let document = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<w:document xmlns:w=\"{W}\" xmlns:r=\"{R}\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\"><w:body>{body}<w:sectPr><w:pgSz w:w=\"12240\" w:h=\"15840\"/><w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\" w:header=\"720\" w:footer=\"720\" w:gutter=\"0\"/></w:sectPr></w:body></w:document>"
    );
    let rels = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{}</Relationships>",
        writer.rels.join("")
    );
    let core = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<cp:coreProperties xmlns:cp=\"http://schemas.openxmlformats.org/package/2006/metadata/core-properties\" xmlns:dc=\"http://purl.org/dc/elements/1.1/\"><dc:title>{}</dc:title><dc:creator>{}</dc:creator></cp:coreProperties>",
        xml_text(doc.title.as_deref().unwrap_or("")),
        xml_text(doc.author.as_deref().unwrap_or(""))
    );
    let mut zip_out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    let mut add = |name: &str, data: &[u8]| -> Result<()> {
        zip_out
            .start_file(name, options)
            .map_err(|e| bad("docx", e))?;
        zip_out.write_all(data).map_err(|e| bad("docx", e))
    };
    add("[Content_Types].xml", CONTENT_TYPES.as_bytes())?;
    add("_rels/.rels", ROOT_RELS.as_bytes())?;
    add("docProps/core.xml", core.as_bytes())?;
    add("word/document.xml", document.as_bytes())?;
    add("word/_rels/document.xml.rels", rels.as_bytes())?;
    add("word/styles.xml", styles().as_bytes())?;
    add("word/numbering.xml", numbering_xml().as_bytes())?;
    for (name, bytes) in &writer.media {
        add(&format!("word/media/{name}"), bytes)?;
    }
    Ok(zip_out.finish().map_err(|e| bad("docx", e))?.into_inner())
}
