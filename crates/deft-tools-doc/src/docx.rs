//! Word documents: a reader for the common structures and a small writer.

use crate::zipped::{Package, join};
use crate::{Block, Document, Image, Merge, Span, bad};
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

/// Text boxes directly in a paragraph. Word writes each one twice, in
/// `mc:Choice` and again in `mc:Fallback`, so the fallback is skipped.
fn text_boxes<'a, 'i>(p: Node<'a, 'i>) -> Vec<Node<'a, 'i>> {
    p.descendants()
        .filter(|d| d.tag_name().name() == "txbxContent")
        .filter(|d| {
            d.ancestors()
                .skip(1)
                .take_while(|a| *a != p)
                .all(|a| !matches!(a.tag_name().name(), "txbxContent" | "Fallback"))
        })
        .collect()
}

/// Every paragraph under a node: through nested tables, content controls and
/// text boxes, in reading order.
fn paragraphs_in<'a, 'i>(node: Node<'a, 'i>) -> Vec<Node<'a, 'i>> {
    node.descendants()
        .filter(|d| d.tag_name().name() == "p" && d.tag_name().namespace() == Some(W))
        .filter(|d| !d.ancestors().any(|a| a.tag_name().name() == "Fallback"))
        .collect()
}

/// Add a list item `level` lists deep, under the last item of the list above.
/// `number` is the item's number, when the list is ordered; a number that
/// does not follow on starts a new list.
pub(crate) fn add_item(
    list: &mut Vec<Block>,
    level: usize,
    ordered: bool,
    number: Option<u64>,
    item: Vec<Block>,
) {
    if level > 0
        && let Some(Block::List { items, .. }) = list.last_mut()
        && let Some(parent) = items.last_mut()
    {
        return add_item(parent, level - 1, ordered, number, item);
    }
    if let Some(Block::List {
        ordered: o,
        start,
        items,
    }) = list.last_mut()
        && *o == ordered
        && number.is_none_or(|n| n == *start + items.len() as u64)
    {
        items.push(item);
        return;
    }
    list.push(Block::List {
        ordered,
        start: number.unwrap_or(1),
        items: vec![item],
    });
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
    styles: Styles,
    numbering: Numbering,
    /// Counters by numbering key, one per level.
    counters: HashMap<String, Vec<u64>>,
    images: Vec<Image>,
    seen: HashMap<String, String>,
    /// Notes referred to so far, as (element name, id), in reading order.
    notes: Vec<(String, String)>,
    /// A drop capital waiting for the paragraph it belongs to.
    dropcap: Vec<Span>,
    /// Styles whose paragraphs mostly read as headings, not sentences.
    heading_like_styles: std::collections::HashSet<String>,
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

    /// The level format of list `id` at `level`, the number Word shows
    /// there, and its whole label, such as "2.1" or "(a)". Lists sharing a
    /// format count on together, unless one restarts.
    fn number(&mut self, id: &str, level: usize) -> Option<(Level, u64, String)> {
        let (abs, levels) = self.numbering.nums.get(id)?;
        let lvl = levels
            .get(level)
            .or(levels.first())
            .cloned()
            .unwrap_or_default();
        let restart = self
            .numbering
            .restarts
            .get(id)
            .and_then(|r| r.get(&level))
            .copied();
        let key = if restart.is_some() {
            format!("num {id}")
        } else {
            format!("abstract {abs}")
        };
        let counters = self.counters.entry(key).or_default();
        if counters.len() <= level {
            counters.resize(level + 1, 0);
        }
        let n = if counters[level] == 0 {
            restart.unwrap_or(lvl.start)
        } else {
            counters[level] + 1
        };
        counters[level] = n;
        // A deeper level starts again under each new item.
        counters.truncate(level + 1);
        let mut label = lvl.text.clone();
        for (k, upper) in levels.iter().enumerate().take(9).rev() {
            let mark = format!("%{}", k + 1);
            if !label.contains(&mark) {
                continue;
            }
            let value = counters
                .get(k)
                .copied()
                .filter(|v| *v > 0)
                .unwrap_or(upper.start);
            let format = if lvl.legal && k < level {
                "decimal"
            } else {
                upper.format.as_str()
            };
            label = label.replace(&mark, &format_number(value, format));
        }
        Some((lvl, n, label.trim().to_string()))
    }

    fn run(&mut self, r: Node, link: Option<&str>, out: &mut Vec<Span>) {
        let rpr = w(r, "rPr");
        let (bold, italic, strike) = (flag(rpr, "b"), flag(rpr, "i"), flag(rpr, "strike"));
        let code = rpr
            .and_then(|p| w(p, "rFonts"))
            .and_then(|f| f.attribute((W, "ascii")))
            .is_some_and(|f| f.contains("Courier") || f.contains("Consolas") || f.contains("Mono"));
        let style = (bold, italic, code, strike);
        let mut text = String::new();
        for c in r.children().filter(Node::is_element) {
            match c.tag_name().name() {
                "t" => text.push_str(c.text().unwrap_or("")),
                "tab" => text.push('\t'),
                "br" if c.attribute((W, "type")) != Some("page") => text.push('\n'),
                "drawing" | "pict" => self.pictures(c, out),
                // Word writes the same thing twice here; the first is enough.
                "AlternateContent" => {
                    let choice = c.children().find(|x| x.tag_name().name() == "Choice");
                    let chosen = choice
                        .filter(|x| x.descendants().any(|d| d.tag_name().name() == "drawing"));
                    let chosen =
                        chosen.or(c.children().find(|x| x.tag_name().name() == "Fallback"));
                    for d in chosen.iter().flat_map(|x| x.children()) {
                        if matches!(d.tag_name().name(), "drawing" | "pict") {
                            self.pictures(d, out);
                        }
                    }
                }
                // A legacy form check box shows its state.
                "fldChar" => {
                    if let Some(b) = c.descendants().find(|d| d.tag_name().name() == "checkBox") {
                        let state = w(b, "checked").or_else(|| w(b, "default"));
                        let on = state.is_some_and(|s| {
                            !matches!(s.attribute((W, "val")), Some("0" | "false"))
                                && (s.tag_name().name() == "checked"
                                    || s.attribute((W, "val")).is_some())
                        });
                        text.push(if on { '☒' } else { '☐' });
                    }
                }
                kind @ ("footnoteReference" | "endnoteReference") => {
                    if let Some(id) = c.attribute((W, "id")) {
                        // Whatever came before the marker stays before it.
                        self.flush(&mut text, &style, link, out);
                        let name = kind.trim_end_matches("Reference").to_string();
                        self.notes.push((name, id.to_string()));
                        out.push(Span::note(format!("[{}]", self.notes.len())));
                    }
                }
                _ => {}
            }
        }
        self.flush(&mut text, &style, link, out);
    }

    /// End the current piece of a run's text.
    fn flush(
        &self,
        text: &mut String,
        style: &(bool, bool, bool, bool),
        link: Option<&str>,
        out: &mut Vec<Span>,
    ) {
        if !text.is_empty() {
            out.push(Span {
                text: std::mem::take(text),
                bold: style.0,
                italic: style.1,
                code: style.2,
                strike: style.3,
                link: link.map(String::from),
                ..Default::default()
            });
        }
    }

    /// The pictures of a drawing, leaving those inside its text boxes, which
    /// are read as paragraphs of their own.
    fn pictures(&mut self, drawing: Node, out: &mut Vec<Span>) {
        let ids: Vec<&str> = drawing
            .descendants()
            .filter(|d| !d.ancestors().any(|a| a.tag_name().name() == "txbxContent"))
            .filter_map(|d| match d.tag_name().name() {
                "blip" if d.tag_name().namespace() == Some(A) => d.attribute((R, "embed")),
                "imagedata" => d.attribute((R, "id")),
                _ => None,
            })
            .collect();
        for id in ids {
            if let Some(name) = self.image(id) {
                out.push(Span {
                    image: Some(name),
                    ..Default::default()
                });
            }
        }
    }

    /// Inline content, through the containers that may wrap runs.
    fn inline(&mut self, parent: Node, link: Option<&str>, out: &mut Vec<Span>) {
        for c in parent.children().filter(Node::is_element) {
            match c.tag_name().name() {
                "r" => self.run(c, link, out),
                "hyperlink" => {
                    let target = c
                        .attribute((R, "id"))
                        .and_then(|id| self.rels.get(id))
                        .map(|(t, _)| t.clone());
                    self.inline(c, target.as_deref().or(link), out);
                }
                "ins" | "moveTo" | "smartTag" | "sdt" | "sdtContent" | "fldSimple" => {
                    self.inline(c, link, out);
                }
                _ => {}
            }
        }
    }

    fn spans(&mut self, p: Node) -> Vec<Span> {
        let mut out = Vec::new();
        self.inline(p, None, &mut out);
        out
    }

    /// All the text under a node, its paragraphs joined by `between`: a
    /// line break in a table cell, a space in a note.
    fn flat(&mut self, node: Node, between: &str) -> Vec<Span> {
        let mut out = Vec::new();
        for p in paragraphs_in(node) {
            let mut spans = self.spans(p);
            // A paragraph's own indent is layout.
            if let Some(first) = spans.first_mut()
                && first.image.is_none()
            {
                first.text = first.text.trim_start_matches([' ', '\t']).to_string();
            }
            spans.retain(|s| s.image.is_some() || s.note || !s.text.is_empty());
            if spans.is_empty() {
                continue;
            }
            if let Some(label) = self.label_of(p) {
                spans.insert(0, Span::label(format!("{label} ")));
            }
            if !out.is_empty() {
                out.push(Span::plain(between));
            }
            out.extend(spans);
        }
        out
    }

    /// The number Word shows before a paragraph in a cell, if it has one
    /// that is not a bullet.
    fn label_of(&mut self, p: Node) -> Option<String> {
        let (id, level) = self.numbered(p)?;
        let (lvl, _, label) = self.number(&id, level)?;
        (lvl.ordered && !label.is_empty()).then_some(label)
    }

    /// The list and level a paragraph is numbered with, directly or through
    /// its style.
    fn numbered(&self, p: Node) -> Option<(String, usize)> {
        let ppr = w(p, "pPr");
        let style = ppr
            .and_then(|x| w(x, "pStyle"))
            .and_then(|s| s.attribute((W, "val")))
            .unwrap_or("");
        let own = ppr.and_then(|x| w(x, "numPr"));
        let val = |n: Option<Node>, name: &str| {
            n.and_then(|x| w(x, name))
                .and_then(|x| x.attribute((W, "val")))
                .map(String::from)
        };
        let from_style = self.styles.numbering(style);
        let id = val(own, "numId").or_else(|| from_style.as_ref().map(|s| s.0.clone()))?;
        if id == "0" || id.is_empty() {
            return None;
        }
        let level = val(own, "ilvl")
            .and_then(|v| v.parse().ok())
            .or_else(|| from_style.as_ref().and_then(|s| s.1))
            .or_else(|| self.numbering.style_levels.get(style).copied())
            .unwrap_or(0);
        Some((id, level.min(8)))
    }

    /// Rows of a table or cells of a row, also when a content control wraps them.
    fn parts_of<'a, 'i>(&self, parent: Node<'a, 'i>, name: &str) -> Vec<Node<'a, 'i>> {
        let mut out = Vec::new();
        for c in parent.children().filter(Node::is_element) {
            match c.tag_name().name() {
                "sdt" | "sdtContent" => out.extend(self.parts_of(c, name)),
                n if n == name => out.push(c),
                _ => {}
            }
        }
        out
    }

    /// Block content: the body, a content control or a text box.
    fn blocks(&mut self, parent: Node, out: &mut Vec<Block>) {
        for node in parent.children().filter(Node::is_element) {
            match node.tag_name().name() {
                "p" => self.paragraph(node, out),
                "tbl" => {
                    let t = self.table(node);
                    out.push(t);
                }
                "sdt" | "sdtContent" | "customXml" => self.blocks(node, out),
                _ => {}
            }
        }
    }

    /// The page header of the first page, when it holds more than page
    /// numbers: a résumé or a letterhead keeps its name and contact details
    /// there. Footers, mostly page numbers and notices, are left out.
    fn header(&mut self, doc: &Xml, blocks: &mut Vec<Block>) {
        let Some(sect) = doc
            .descendants()
            .find(|n| n.tag_name().name() == "sectPr" && n.tag_name().namespace() == Some(W))
        else {
            return;
        };
        let title_page = w(sect, "titlePg")
            .is_some_and(|t| !matches!(t.attribute((W, "val")), Some("0" | "false")));
        let of_kind = |kind: &str| {
            sect.children()
                .filter(|c| c.tag_name().name() == "headerReference")
                .find(|r| r.attribute((W, "type")).unwrap_or("default") == kind)
                .and_then(|r| r.attribute((R, "id")))
                .and_then(|id| self.rels.get(id))
                .map(|(target, _)| join("word/document.xml", target))
        };
        let mut parts = Vec::new();
        if title_page {
            parts.extend(of_kind("first"));
        }
        parts.extend(of_kind("default"));
        let document_rels = std::mem::take(&mut self.rels);
        for part in parts {
            let Some(text) = self.pkg.text(&part) else {
                continue;
            };
            let Ok(xml) = Xml::parse(&text) else {
                continue;
            };
            self.rels = relationships(self.pkg, &part);
            let mut found = Vec::new();
            self.header_blocks(xml.root_element(), &mut found);
            if !found.is_empty() {
                blocks.extend(found);
                break;
            }
        }
        self.rels = document_rels;
    }

    fn header_blocks(&mut self, parent: Node, out: &mut Vec<Block>) {
        for node in parent.children().filter(Node::is_element) {
            match node.tag_name().name() {
                // A page number, and text boxes, which hold watermarks.
                "p" if !page_field(node) => self.own_text(node, out),
                "tbl" => {
                    let t = self.table(node);
                    let empty = matches!(&t, Block::Table(rows, _)
                        if rows.iter().flatten().flatten().all(|s| s.text.trim().is_empty() && s.image.is_none()));
                    if !empty {
                        out.push(t);
                    }
                }
                "sdt" | "sdtContent" | "customXml" => self.header_blocks(node, out),
                _ => {}
            }
        }
    }

    /// The notes referred to, after a rule. The list can grow as it is read,
    /// because a note may refer to another.
    fn notes(&mut self, blocks: &mut Vec<Block>) {
        if self.notes.is_empty() {
            return;
        }
        let read = |part: &str| self.pkg.text(part).unwrap_or_default();
        let (foot, end) = (read("word/footnotes.xml"), read("word/endnotes.xml"));
        let (foot_xml, end_xml) = (Xml::parse(&foot).ok(), Xml::parse(&end).ok());
        let mut part = "";
        let mut at = 0;
        blocks.push(Block::Rule);
        while at < self.notes.len() {
            let (kind, id) = self.notes[at].clone();
            at += 1;
            let (name, xml) = if kind == "footnote" {
                ("word/footnotes.xml", &foot_xml)
            } else {
                ("word/endnotes.xml", &end_xml)
            };
            if part != name {
                part = name;
                self.rels = relationships(self.pkg, name);
            }
            let found = xml.as_ref().and_then(|x| {
                x.descendants().find(|n| {
                    n.tag_name().name() == kind && n.attribute((W, "id")) == Some(id.as_str())
                })
            });
            let Some(note) = found else { continue };
            let mut spans = vec![Span::note(format!("[{at}]:")), Span::plain(" ")];
            let mut body = self.flat(note, " ");
            // Word starts a note with a space after its number.
            if let Some(first) = body.first_mut() {
                first.text = first.text.trim_start().to_string();
            }
            spans.extend(body);
            blocks.push(Block::Paragraph(spans));
        }
    }

    /// A table on its grid: w:gridSpan merges across, w:vMerge down.
    fn table(&mut self, t: Node) -> Block {
        fn val<'a>(n: Option<Node<'a, '_>>) -> Option<&'a str> {
            n.and_then(|x| x.attribute((W, "val")))
        }
        let count = |n: Option<Node>| val(n).and_then(|v| v.parse::<usize>().ok());
        let mut rows: Vec<Vec<Vec<Span>>> = Vec::new();
        let mut merges: Vec<Merge> = Vec::new();
        // Grid column -> the merge a vertical run started there belongs to.
        let mut down: HashMap<usize, usize> = HashMap::new();
        for (r, tr) in self.parts_of(t, "tr").into_iter().enumerate() {
            let trpr = w(tr, "trPr");
            let mut row: Vec<Vec<Span>> = Vec::new();
            for _ in 0..count(trpr.and_then(|p| w(p, "gridBefore")))
                .unwrap_or(0)
                .min(64)
            {
                row.push(vec![]);
            }
            for tc in self.parts_of(tr, "tc") {
                let tcpr = w(tc, "tcPr");
                let span = count(tcpr.and_then(|p| w(p, "gridSpan")))
                    .unwrap_or(1)
                    .clamp(1, 64);
                let col = row.len();
                let vmerge = tcpr.and_then(|p| w(p, "vMerge"));
                let continues = vmerge.is_some() && val(vmerge) != Some("restart");
                if continues && let Some(&m) = down.get(&col) {
                    merges[m].rows += 1;
                    row.extend(std::iter::repeat_n(vec![], span));
                    continue;
                }
                for c in col..col + span {
                    down.remove(&c);
                }
                if vmerge.is_some() || span > 1 {
                    if vmerge.is_some() {
                        down.insert(col, merges.len());
                    }
                    merges.push(Merge {
                        row: r,
                        col,
                        rows: 1,
                        cols: span,
                    });
                }
                row.push(self.flat(tc, "\n"));
                row.extend(std::iter::repeat_n(vec![], span - 1));
            }
            for _ in 0..count(trpr.and_then(|p| w(p, "gridAfter")))
                .unwrap_or(0)
                .min(64)
            {
                row.push(vec![]);
            }
            rows.push(row);
        }
        merges.retain(|m| m.rows * m.cols > 1);
        Block::Table(rows, merges)
    }

    fn paragraph(&mut self, p: Node, blocks: &mut Vec<Block>) {
        self.own_text(p, blocks);
        for text_box in text_boxes(p) {
            self.blocks(text_box, blocks);
        }
    }

    fn own_text(&mut self, p: Node, blocks: &mut Vec<Block>) {
        let ppr = w(p, "pPr");
        let style = ppr
            .and_then(|x| w(x, "pStyle"))
            .and_then(|s| s.attribute((W, "val")))
            .unwrap_or("");
        let breaks_page = p
            .descendants()
            .any(|d| d.tag_name().name() == "br" && d.attribute((W, "type")) == Some("page"));
        let mut spans = self.spans(p);
        if breaks_page {
            blocks.push(Block::PageBreak);
        }
        if spans.is_empty() {
            return;
        }
        // A drop capital is a paragraph of its own: it starts the next one.
        let dropcap = ppr
            .and_then(|x| w(x, "framePr"))
            .and_then(|f| f.attribute((W, "dropCap")))
            .is_some_and(|d| d != "none");
        if dropcap {
            self.dropcap.extend(spans);
            return;
        }
        if !self.dropcap.is_empty() {
            let mut first = std::mem::take(&mut self.dropcap);
            first.append(&mut spans);
            spans = first;
        }
        // A paragraph's own indent is layout, and Markdown would read it as code.
        if let Some(first) = spans.first_mut()
            && first.image.is_none()
        {
            first.text = first.text.trim_start_matches([' ', '\t']).to_string();
        }
        spans.retain(|s| s.image.is_some() || s.note || !s.text.is_empty());
        if spans.is_empty() {
            return;
        }
        let own_outline = ppr
            .and_then(|x| w(x, "outlineLvl"))
            .and_then(|o| o.attribute((W, "val")))
            .and_then(|v| v.parse::<u8>().ok())
            .filter(|v| *v < 9);
        // An outline level makes a heading in Word's navigation pane. A
        // style that sets one for clauses of running text, as contracts do
        // for "1.1", is not a heading style: most of its paragraphs read as
        // sentences.
        let text: String = spans.iter().map(|s| s.text.as_str()).collect();
        let heading = match (self.styles.heading(style), own_outline) {
            (Some((level, true)), _) => Some(level),
            (_, Some(own)) if headline_like(&text) => Some(own + 1),
            (Some((level, false)), None) if self.heading_like_styles.contains(style) => Some(level),
            _ => None,
        };
        // A list the numbering part does not define shows bullets.
        let numbered = self.numbered(p).map(|(id, level)| {
            let (lvl, n, label) = self.number(&id, level).unwrap_or_default();
            (level, lvl, n, label)
        });
        if let Some(level) = heading {
            if let Some((_, lvl, _, label)) = &numbered
                && lvl.ordered
                && !label.is_empty()
            {
                spans.insert(0, Span::label(format!("{label} ")));
            }
            blocks.push(Block::Heading(level.clamp(1, 6), spans));
            return;
        }
        if let Some((level, lvl, n, label)) = numbered {
            // A plain "1." numbers a Markdown list; "2.1", "(a)" or "Article
            // 3" stay with the paragraph as its text.
            if !lvl.ordered {
                add_item(blocks, level, false, None, vec![Block::Paragraph(spans)]);
            } else if lvl.format == "decimal" && lvl.text == format!("%{}.", level + 1) {
                add_item(blocks, level, true, Some(n), vec![Block::Paragraph(spans)]);
            } else {
                if !label.is_empty() {
                    spans.insert(0, Span::label(format!("{label} ")));
                }
                blocks.push(Block::Paragraph(spans));
            }
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

/// Whether text reads as a heading: short, and not ending as a sentence or
/// a lead-in does.
fn headline_like(text: &str) -> bool {
    let t = text.trim();
    !t.is_empty()
        && t.chars().count() <= 120
        && !t.ends_with(['.', ';', ':', ',', '。', '；', '：', '，'])
}

/// The styles of the document whose paragraphs are mostly heading-like.
fn heading_like_styles(doc: &Xml) -> std::collections::HashSet<String> {
    let mut counts: HashMap<String, (usize, usize)> = HashMap::new();
    for p in doc
        .descendants()
        .filter(|d| d.tag_name().name() == "p" && d.tag_name().namespace() == Some(W))
    {
        let Some(style) = w(p, "pPr")
            .and_then(|x| w(x, "pStyle"))
            .and_then(|s| s.attribute((W, "val")))
        else {
            continue;
        };
        let text: String = p
            .descendants()
            .filter(|d| d.tag_name().name() == "t")
            .filter_map(|d| d.text())
            .collect();
        if text.trim().is_empty() {
            continue;
        }
        let entry = counts.entry(style.to_string()).or_default();
        entry.0 += 1;
        entry.1 += usize::from(headline_like(&text));
    }
    counts
        .into_iter()
        .filter(|(_, (all, like))| *like * 10 >= *all * 8)
        .map(|(s, _)| s)
        .collect()
}

/// Whether a paragraph shows a page number or count.
fn page_field(p: Node) -> bool {
    p.descendants().any(|d| {
        let code = match d.tag_name().name() {
            "instrText" => d.text().unwrap_or(""),
            "fldSimple" => d.attribute((W, "instr")).unwrap_or(""),
            _ => return false,
        };
        let code = code.trim_start().to_ascii_uppercase();
        code.starts_with("PAGE") || code.starts_with("NUMPAGES") || code.starts_with("SECTIONPAGES")
    })
}

/// What the styles of styles.xml say about headings and numbering.
#[derive(Default)]
struct Styles {
    /// Style id -> (name, based on, numbering (numId, level), outline level).
    defs: HashMap<String, StyleDef>,
}

#[derive(Default)]
struct StyleDef {
    name: String,
    based_on: Option<String>,
    num: Option<(String, Option<usize>)>,
    outline: Option<u8>,
}

impl Styles {
    /// The style and the styles it is based on, nearest first.
    fn chain(&self, id: &str) -> Vec<&StyleDef> {
        let mut out = Vec::new();
        let mut at = Some(id.to_string());
        while let Some(id) = at {
            let Some(def) = self.defs.get(&id) else { break };
            if out.len() > 20 {
                break;
            }
            out.push(def);
            at = def.based_on.clone();
        }
        out
    }

    /// The heading level of a style, and whether its name says so ("heading
    /// 2", "Title") rather than only its outline level.
    fn heading(&self, id: &str) -> Option<(u8, bool)> {
        let by_name = |name: &str| -> Option<u8> {
            let lower = name.to_lowercase();
            if lower == "title" {
                return Some(1);
            }
            lower.strip_prefix("heading")?.trim().parse().ok()
        };
        let chain = self.chain(id);
        // By its name, or by its id when styles.xml does not have it.
        let named = match chain.first() {
            Some(def) => by_name(&def.name).or_else(|| by_name(id)),
            None => by_name(id),
        };
        if let Some(n) = named.filter(|n| (1..=9).contains(n)) {
            return Some((n, true));
        }
        chain
            .iter()
            .find_map(|d| d.outline)
            .filter(|o| *o < 9)
            .map(|o| (o + 1, false))
    }

    /// The list a style numbers its paragraphs with, through its base styles.
    fn numbering(&self, id: &str) -> Option<(String, Option<usize>)> {
        self.chain(id).iter().find_map(|d| d.num.clone())
    }
}

fn read_styles(pkg: &Package) -> Styles {
    let mut out = Styles::default();
    let Some(text) = pkg.text("word/styles.xml") else {
        return out;
    };
    let Ok(doc) = Xml::parse(&text) else {
        return out;
    };
    let val = |n: Option<Node>| n.and_then(|x| x.attribute((W, "val"))).map(String::from);
    for s in doc.descendants().filter(|n| n.tag_name().name() == "style") {
        let Some(id) = s.attribute((W, "styleId")) else {
            continue;
        };
        let ppr = w(s, "pPr");
        let num = ppr.and_then(|p| w(p, "numPr")).and_then(|n| {
            let id = val(w(n, "numId"))?;
            Some((id, val(w(n, "ilvl")).and_then(|v| v.parse().ok())))
        });
        out.defs.insert(
            id.to_string(),
            StyleDef {
                name: val(w(s, "name")).unwrap_or_default(),
                based_on: val(w(s, "basedOn")),
                num,
                outline: val(ppr.and_then(|p| w(p, "outlineLvl"))).and_then(|v| v.parse().ok()),
            },
        );
    }
    out
}

/// One level of a list format.
#[derive(Clone, Default)]
struct Level {
    ordered: bool,
    start: u64,
    /// Word's number format, such as "decimal", "lowerLetter" or "bullet".
    format: String,
    /// The label with %1, %2 ... for the numbers of each level, as "%1.%2".
    text: String,
    /// Legal numbering: the numbers of the levels above are decimal.
    legal: bool,
}

/// Numbering definitions from numbering.xml.
#[derive(Default)]
struct Numbering {
    /// numId -> (abstractNumId, levels).
    nums: HashMap<String, (String, Vec<Level>)>,
    /// numId -> level -> start value that restarts the list there.
    restarts: HashMap<String, HashMap<usize, u64>>,
    /// Style id -> the level of a list that names the style.
    style_levels: HashMap<String, usize>,
}

/// A number in one of Word's number formats.
fn format_number(n: u64, format: &str) -> String {
    let letters = |upper: bool| {
        // a ... z, then aa, bb ...
        let base = if upper { b'A' } else { b'a' };
        let n = n.max(1) - 1;
        let letter = (base + (n % 26) as u8) as char;
        letter.to_string().repeat((n / 26 + 1) as usize)
    };
    let roman = |upper: bool| {
        let mut n = n;
        let mut out = String::new();
        for (v, s) in [
            (1000, "m"),
            (900, "cm"),
            (500, "d"),
            (400, "cd"),
            (100, "c"),
            (90, "xc"),
            (50, "l"),
            (40, "xl"),
            (10, "x"),
            (9, "ix"),
            (5, "v"),
            (4, "iv"),
            (1, "i"),
        ] {
            while n >= v {
                out.push_str(s);
                n -= v;
            }
        }
        if upper { out.to_uppercase() } else { out }
    };
    let chinese = |n: u64| {
        const D: [&str; 10] = ["〇", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
        if n >= 100 {
            return n.to_string();
        }
        let (tens, ones) = ((n / 10) as usize, (n % 10) as usize);
        match (tens, ones) {
            (0, o) => D[o].to_string(),
            (1, 0) => "十".to_string(),
            (1, o) => format!("十{}", D[o]),
            (t, 0) => format!("{}十", D[t]),
            (t, o) => format!("{}十{}", D[t], D[o]),
        }
    };
    match format {
        "lowerLetter" => letters(false),
        "upperLetter" => letters(true),
        "lowerRoman" => roman(false),
        "upperRoman" => roman(true),
        "decimalZero" if n < 10 => format!("0{n}"),
        "decimalEnclosedCircle" | "decimalEnclosedCircleChinese" if (1..=20).contains(&n) => {
            char::from_u32(0x245F + n as u32).map_or(n.to_string(), String::from)
        }
        "decimalFullWidth" | "decimalFullWidth2" => n
            .to_string()
            .chars()
            .map(|c| char::from_u32(c as u32 - '0' as u32 + 0xFF10).unwrap_or(c))
            .collect(),
        "chineseCounting"
        | "chineseCountingThousand"
        | "japaneseCounting"
        | "ideographTraditional"
        | "taiwaneseCounting"
        | "koreanCounting" => chinese(n),
        "none" | "bullet" => String::new(),
        _ => n.to_string(),
    }
}

fn numbering(pkg: &Package) -> Numbering {
    let mut out = Numbering::default();
    let Some(text) = pkg.text("word/numbering.xml") else {
        return out;
    };
    let Ok(doc) = Xml::parse(&text) else {
        return out;
    };
    let number = |n: Option<Node>| {
        n.and_then(|x| x.attribute((W, "val")))
            .and_then(|v| v.parse::<u64>().ok())
    };
    let val = |n: Option<Node>| n.and_then(|x| x.attribute((W, "val"))).map(String::from);
    let level = |l: Node, at: usize| {
        let format = val(w(l, "numFmt")).unwrap_or_else(|| "decimal".into());
        Level {
            ordered: !matches!(format.as_str(), "bullet" | "none"),
            start: number(w(l, "start")).unwrap_or(1),
            // Without a label of its own a level shows its number.
            text: val(w(l, "lvlText")).unwrap_or_else(|| format!("%{}.", at + 1)),
            legal: w(l, "isLgl").is_some_and(|x| x.attribute((W, "val")) != Some("0")),
            format,
        }
    };
    let mut abstracts = HashMap::new();
    for a in doc
        .descendants()
        .filter(|n| n.tag_name().name() == "abstractNum")
    {
        let id = a.attribute((W, "abstractNumId")).unwrap_or("");
        let mut levels: Vec<Level> = Vec::new();
        for l in a.children().filter(|c| c.tag_name().name() == "lvl") {
            let at = l
                .attribute((W, "ilvl"))
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(levels.len())
                .min(8);
            if let Some(style) = val(w(l, "pStyle")) {
                out.style_levels.insert(style, at);
            }
            if levels.len() <= at {
                levels.resize(at + 1, Level::default());
            }
            levels[at] = level(l, at);
        }
        abstracts.insert(id.to_string(), levels);
    }
    // A list format can be a numbering style, whose levels are defined by the
    // format that links to that style.
    let linked: HashMap<String, String> = doc
        .descendants()
        .filter(|n| n.tag_name().name() == "abstractNum")
        .filter_map(|a| {
            Some((
                val(w(a, "styleLink"))?,
                a.attribute((W, "abstractNumId"))?.to_string(),
            ))
        })
        .collect();
    for a in doc
        .descendants()
        .filter(|n| n.tag_name().name() == "abstractNum")
    {
        let id = a.attribute((W, "abstractNumId")).unwrap_or("");
        if let Some(link) = val(w(a, "numStyleLink"))
            && let Some(source) = linked.get(&link)
            && abstracts.get(id).is_none_or(Vec::is_empty)
            && let Some(levels) = abstracts.get(source).cloned()
        {
            abstracts.insert(id.to_string(), levels);
        }
    }
    for n in doc.descendants().filter(|n| n.tag_name().name() == "num") {
        let id = n.attribute((W, "numId")).unwrap_or("").to_string();
        let abs = w(n, "abstractNumId")
            .and_then(|x| x.attribute((W, "val")))
            .unwrap_or("")
            .to_string();
        let mut levels = abstracts.get(&abs).cloned().unwrap_or_default();
        for o in n
            .children()
            .filter(|c| c.tag_name().name() == "lvlOverride")
        {
            let at = o
                .attribute((W, "ilvl"))
                .and_then(|v| v.parse().ok())
                .unwrap_or(0usize)
                .min(8);
            if let Some(start) = number(w(o, "startOverride")) {
                out.restarts
                    .entry(id.clone())
                    .or_default()
                    .insert(at, start);
            }
            // A list can bring its own format for a level.
            if let Some(l) = w(o, "lvl") {
                if levels.len() <= at {
                    levels.resize(at + 1, Level::default());
                }
                levels[at] = level(l, at);
            }
        }
        out.nums.insert(id, (abs, levels));
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
    // Only one body is valid, but some writers make several; Word reads all.
    let bodies: Vec<Node> = doc
        .root_element()
        .children()
        .filter(|c| c.tag_name().name() == "body" && c.tag_name().namespace() == Some(W))
        .collect();
    if bodies.is_empty() {
        return Err(bad("docx", "no document body"));
    }
    let mut reader = Reader {
        pkg: &pkg,
        rels: relationships(&pkg, "word/document.xml"),
        styles: read_styles(&pkg),
        numbering: numbering(&pkg),
        counters: HashMap::new(),
        images: vec![],
        seen: HashMap::new(),
        notes: vec![],
        dropcap: vec![],
        heading_like_styles: heading_like_styles(&doc),
    };
    let mut blocks = Vec::new();
    reader.header(&doc, &mut blocks);
    for body in bodies {
        reader.blocks(body, &mut blocks);
    }
    reader.notes(&mut blocks);
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
            || name == "word/footnotes.xml"
            || name == "word/endnotes.xml"
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

/// Replace paragraph by paragraph. Paragraphs nest, because a text box sits
/// inside one, so each stretch between two paragraph tags is taken alone;
/// otherwise the text after a text box would be missed.
fn replace_part(xml: &str, find: &str, with: &str) -> Result<(String, usize)> {
    let t =
        regex::Regex::new(r"(<w:t(?:\s[^>]*)?>)([^<]*)(</w:t>)").map_err(|e| bad("regex", e))?;
    // Text is matched between these edges: paragraphs, and the two copies
    // of a shape, so that no stretch is half in either copy.
    let edge = regex::Regex::new(
        r"<w:p(?:\s[^>]*)?/?>|</w:p>|<mc:(?:AlternateContent|Choice|Fallback)\b[^>]*>|</mc:(?:AlternateContent|Choice|Fallback)>",
    )
    .map_err(|e| bad("regex", e))?;
    // Word writes shapes twice, the second time in mc:Fallback for old
    // readers. Both copies change, but only the first is counted.
    let fallback =
        regex::Regex::new(r"<mc:Fallback\b|</mc:Fallback>").map_err(|e| bad("regex", e))?;
    let mut copies: Vec<(usize, usize)> = Vec::new();
    let (mut depth, mut opened) = (0usize, 0usize);
    for m in fallback.find_iter(xml) {
        if m.as_str().starts_with("</") {
            depth = depth.saturating_sub(1);
            if depth == 0 {
                copies.push((opened, m.end()));
            }
        } else {
            if depth == 0 {
                opened = m.start();
            }
            depth += 1;
        }
    }
    let counted = |at: usize| !copies.iter().any(|(a, b)| (*a..*b).contains(&at));
    let mut total = 0;
    let mut out = String::with_capacity(xml.len());
    let mut last = 0;
    for m in edge.find_iter(xml) {
        let (text, n) = replace_stretch(&xml[last..m.start()], &t, find, with);
        out.push_str(&text);
        out.push_str(m.as_str());
        if counted(last) {
            total += n;
        }
        last = m.end();
    }
    let (text, n) = replace_stretch(&xml[last..], &t, find, with);
    out.push_str(&text);
    if counted(last) {
        total += n;
    }
    Ok((out, total))
}

/// Find `<w:t ...>text</w:t>` spans in one stretch of a paragraph; a match
/// spanning runs lands in the first of them and is removed from the others.
fn replace_stretch(p: &str, t: &regex::Regex, find: &str, with: &str) -> (String, usize) {
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
        return (p.to_string(), 0);
    }
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
    (rebuilt.replace("<w:t>", "<w:t xml:space=\"preserve\">"), n)
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
    /// Heading anchors, which name the bookmarks that `#anchor` links reach.
    anchors: deft_core::text::Anchors,
    bookmark: usize,
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
        if s.strike {
            props.push_str("<w:strike/>");
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

    fn picture(&mut self, name: &str, alt: &str) -> String {
        let Some(image) = self.doc.images.iter().find(|i| i.name == name) else {
            return String::new();
        };
        let file = name.rsplit('/').next().unwrap_or(name).to_string();
        let target = format!("media/{file}");
        let rid = self.rel("image", &target, false);
        // A picture used twice is one file with two references.
        if !self.media.iter().any(|(name, _)| *name == file) {
            self.media.push((file.clone(), &image.bytes));
        }
        // Word shows an SVG through a PNG of it, which older readers use alone.
        let fallback = self.doc.fallbacks.get(name);
        let (blip, size) = match fallback {
            Some(png) => {
                let png_file = format!("{file}.png");
                let png_rid = self.rel("image", &format!("media/{png_file}"), false);
                if !self.media.iter().any(|(name, _)| *name == png_file) {
                    self.media.push((png_file, png));
                }
                let size = image_size(png).map(|(w, h)| (w.div_ceil(2), h.div_ceil(2)));
                (
                    format!(
                        "<a:blip r:embed=\"{png_rid}\"><a:extLst><a:ext uri=\"{{96DAC541-7B7A-43D3-8B79-37D633B846F1}}\"><asvg:svgBlip xmlns:asvg=\"http://schemas.microsoft.com/office/drawing/2016/SVG/main\" r:embed=\"{rid}\"/></a:ext></a:extLst></a:blip>"
                    ),
                    size,
                )
            }
            None => (
                format!("<a:blip r:embed=\"{rid}\"/>"),
                image_size(&image.bytes),
            ),
        };
        self.picture += 1;
        let (w, h) = size.unwrap_or((640, 480));
        // 96 dpi to EMU, limited to the text width of a 6.5 inch column.
        let scale = (5_943_600.0 / (w as f64 * 9525.0)).min(1.0);
        let (cx, cy) = (
            (w as f64 * 9525.0 * scale) as u64,
            (h as f64 * 9525.0 * scale) as u64,
        );
        let n = self.picture;
        format!(
            "<w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:docPr id=\"{n}\" name=\"Picture {n}\" descr=\"{}\"/><a:graphic xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic xmlns:pic=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:nvPicPr><pic:cNvPr id=\"{n}\" name=\"{}\"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill>{blip}<a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r>",
            xml_text(alt),
            xml_text(&file)
        )
    }

    fn spans(&mut self, spans: &[Span]) -> String {
        let mut out = String::new();
        for s in spans {
            if let Some(name) = &s.image {
                out.push_str(&self.picture(name, &s.text));
            } else if let Some(link) = &s.link {
                let run = self.run(s, "<w:color w:val=\"0563C1\"/><w:u w:val=\"single\"/>");
                // A link to a heading of this document goes to its bookmark.
                let target = match link.strip_prefix('#') {
                    Some(anchor) => format!("w:anchor=\"{}\"", xml_text(anchor)),
                    None => format!("r:id=\"{}\"", self.rel("hyperlink", link, true)),
                };
                out.push_str(&format!("<w:hyperlink {target}>{run}</w:hyperlink>"));
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
                let text: String = spans.iter().map(|s| s.text.as_str()).collect();
                let name = self.anchors.next(&text);
                self.bookmark += 1;
                let n = self.bookmark;
                out.push_str(&format!(
                    "<w:p><w:pPr><w:pStyle w:val=\"Heading{}\"/></w:pPr><w:bookmarkStart w:id=\"{n}\" w:name=\"{}\"/>{body}<w:bookmarkEnd w:id=\"{n}\"/></w:p>",
                    (*level).clamp(1, 6),
                    xml_text(&name)
                ));
            }
            Block::Paragraph(spans) => {
                let body = self.spans(spans);
                out.push_str(&format!("<w:p><w:pPr>{indent}</w:pPr>{body}</w:p>"));
            }
            Block::List { ordered, items, .. } => {
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
            Block::Table(rows, _) => {
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

const CONTENT_TYPES: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"png\" ContentType=\"image/png\"/><Default Extension=\"jpg\" ContentType=\"image/jpeg\"/><Default Extension=\"jpeg\" ContentType=\"image/jpeg\"/><Default Extension=\"gif\" ContentType=\"image/gif\"/><Default Extension=\"svg\" ContentType=\"image/svg+xml\"/><Default Extension=\"webp\" ContentType=\"image/webp\"/><Default Extension=\"bmp\" ContentType=\"image/bmp\"/><Default Extension=\"tif\" ContentType=\"image/tiff\"/><Default Extension=\"tiff\" ContentType=\"image/tiff\"/><Default Extension=\"emf\" ContentType=\"image/x-emf\"/><Default Extension=\"wmf\" ContentType=\"image/x-wmf\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/><Override PartName=\"/word/styles.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml\"/><Override PartName=\"/word/numbering.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml\"/><Override PartName=\"/docProps/core.xml\" ContentType=\"application/vnd.openxmlformats-package.core-properties+xml\"/></Types>";

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
        anchors: Default::default(),
        bookmark: 0,
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
