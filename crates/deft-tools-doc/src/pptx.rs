//! PowerPoint: slide text and pictures, in reading order.

use crate::zipped::{Package, join};
use crate::{Block, Document, Image, Span, bad};
use deft_core::error::Result;
use roxmltree::{Document as Xml, Node};
use std::collections::HashMap;

const R: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

/// One slide's content.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Slide {
    pub title: Option<String>,
    pub body: Vec<Vec<Span>>,
    /// Tables: rows of cells.
    pub tables: Vec<Vec<Vec<Vec<Span>>>>,
    pub images: Vec<String>,
    pub notes: Option<String>,
}

fn name(n: &Node) -> String {
    n.tag_name().name().to_string()
}

/// A paragraph's runs. `links` maps relationship ids to web addresses.
fn paragraph_spans(p: Node, links: &HashMap<String, String>) -> Vec<Span> {
    let mut out: Vec<Span> = Vec::new();
    for r in p
        .children()
        .filter(|c| matches!(c.tag_name().name(), "r" | "fld" | "br"))
    {
        if r.tag_name().name() == "br" {
            out.push(Span::plain("\n"));
            continue;
        }
        let props = r.children().find(|c| c.tag_name().name() == "rPr");
        let on = |k: &str| props.and_then(|x| x.attribute(k)) == Some("1");
        let link = props
            .and_then(|x| x.children().find(|c| c.tag_name().name() == "hlinkClick"))
            .and_then(|h| h.attribute((R, "id")))
            .and_then(|id| links.get(id))
            .cloned();
        let text: String = r
            .children()
            .filter(|c| c.tag_name().name() == "t")
            .filter_map(|t| t.text())
            .collect();
        if text.is_empty() {
            continue;
        }
        let span = Span {
            text,
            bold: on("b"),
            italic: on("i"),
            link,
            ..Default::default()
        };
        // PowerPoint splits text into runs at will; equal neighbours are one.
        match out.last_mut() {
            Some(last)
                if last.bold == span.bold
                    && last.italic == span.italic
                    && last.link == span.link
                    && last.text != "\n" =>
            {
                last.text.push_str(&span.text)
            }
            _ => out.push(span),
        }
    }
    out
}

/// Relationship id -> address, for the links of one part that leave the deck.
fn web_links(pkg: &Package, part: &str) -> HashMap<String, String> {
    let (dir, file) = part.rsplit_once('/').unwrap_or(("", part));
    let mut out = HashMap::new();
    if let Some(text) = pkg.text(&format!("{dir}/_rels/{file}.rels"))
        && let Ok(doc) = Xml::parse(&text)
    {
        for r in doc.root_element().children().filter(Node::is_element) {
            if let (Some(id), Some(t), Some("External")) = (
                r.attribute("Id"),
                r.attribute("Target"),
                r.attribute("TargetMode"),
            ) {
                out.insert(id.to_string(), t.to_string());
            }
        }
    }
    out
}

fn rels(pkg: &Package, part: &str) -> HashMap<String, String> {
    let (dir, file) = part.rsplit_once('/').unwrap_or(("", part));
    let mut out = HashMap::new();
    if let Some(text) = pkg.text(&format!("{dir}/_rels/{file}.rels"))
        && let Ok(doc) = Xml::parse(&text)
    {
        for r in doc.root_element().children().filter(Node::is_element) {
            if let (Some(id), Some(t)) = (r.attribute("Id"), r.attribute("Target")) {
                out.insert(id.to_string(), t.to_string());
            }
        }
    }
    out
}

/// Slide part names in presentation order.
fn slide_order(pkg: &Package) -> Result<Vec<String>> {
    let text = pkg
        .text("ppt/presentation.xml")
        .ok_or_else(|| bad("pptx", "no ppt/presentation.xml"))?;
    let doc = Xml::parse(&text).map_err(|e| bad("pptx", e))?;
    let targets = rels(pkg, "ppt/presentation.xml");
    let order = doc
        .descendants()
        .filter(|n| n.tag_name().name() == "sldId")
        .filter_map(|n| n.attribute((R, "id")))
        .filter_map(|id| targets.get(id))
        .map(|t| join("ppt/presentation.xml", t))
        .collect();
    Ok(order)
}

fn read_slide(pkg: &Package, part: &str, images: &mut Vec<Image>) -> Result<Slide> {
    let text = pkg.text(part).ok_or_else(|| bad("pptx", part))?;
    let doc = Xml::parse(&text).map_err(|e| bad("pptx", e))?;
    let targets = rels(pkg, part);
    let links = web_links(pkg, part);
    let mut slide = Slide::default();
    for shape in doc.descendants().filter(|n| name(n) == "sp") {
        let is_title = shape
            .descendants()
            .find(|n| name(n) == "ph")
            .and_then(|p| p.attribute("type"))
            .is_some_and(|t| t == "title" || t == "ctrTitle");
        let mut paragraphs: Vec<Vec<Span>> = shape
            .descendants()
            .filter(|n| name(n) == "p")
            .map(|p| paragraph_spans(p, &links))
            .filter(|s| s.iter().any(|x| !x.text.trim().is_empty()))
            .collect();
        if is_title && slide.title.is_none() {
            let joined: Vec<String> = paragraphs
                .iter()
                .map(|p| p.iter().map(|s| s.text.as_str()).collect())
                .collect();
            let title = joined.join(" ").replace('\n', " ");
            slide.title = Some(title.trim().to_string()).filter(|t| !t.is_empty());
        } else {
            slide.body.append(&mut paragraphs);
        }
    }
    for frame in doc.descendants().filter(|n| name(n) == "relIds") {
        if let Some(path) = frame
            .attribute((R, "dm"))
            .and_then(|id| targets.get(id))
            .map(|t| join(part, t))
        {
            slide.body.extend(diagram_text(pkg, &path));
        }
    }
    for table in doc.descendants().filter(|n| name(n) == "tbl") {
        let rows: Vec<Vec<Vec<Span>>> = table
            .children()
            .filter(|c| c.tag_name().name() == "tr")
            .map(|row| {
                row.children()
                    .filter(|c| c.tag_name().name() == "tc")
                    .map(|c| {
                        // A cell's paragraphs are separate sentences.
                        let mut cell: Vec<Span> = Vec::new();
                        for p in c.descendants().filter(|n| name(n) == "p") {
                            let spans = paragraph_spans(p, &links);
                            if spans.is_empty() {
                                continue;
                            }
                            if !cell.is_empty() {
                                cell.push(Span::plain(" "));
                            }
                            cell.extend(spans);
                        }
                        cell
                    })
                    .collect()
            })
            .collect();
        if !rows.is_empty() {
            slide.tables.push(rows);
        }
    }
    for blip in doc.descendants().filter(|n| name(n) == "blip") {
        if let Some(path) = blip
            .attribute((R, "embed"))
            .and_then(|id| targets.get(id))
            .map(|t| join(part, t))
            && let Some(bytes) = pkg.read(&path)
        {
            let file = path.rsplit('/').next().unwrap_or(&path).to_string();
            if !images.iter().any(|i| i.name == file) {
                images.push(Image {
                    name: file.clone(),
                    bytes,
                });
            }
            slide.images.push(file);
        }
    }
    for target in targets.values().filter(|t| t.contains("notesSlide")) {
        let path = join(part, target);
        if let Some(text) = pkg.text(&path)
            && let Ok(notes) = Xml::parse(&text)
        {
            let lines: Vec<String> = notes
                .descendants()
                .filter(|n| name(n) == "sp")
                .filter(|s| {
                    s.descendants()
                        .find(|n| name(n) == "ph")
                        .and_then(|p| p.attribute("type"))
                        == Some("body")
                })
                .flat_map(|s| {
                    s.descendants()
                        .filter(|n| name(n) == "p")
                        .collect::<Vec<_>>()
                })
                .map(|p| {
                    paragraph_spans(p, &HashMap::new())
                        .iter()
                        .map(|s| s.text.clone())
                        .collect()
                })
                .filter(|l: &String| !l.trim().is_empty())
                .collect();
            if !lines.is_empty() {
                slide.notes = Some(lines.join("\n"));
            }
        }
    }
    Ok(slide)
}

/// The text of a SmartArt diagram, from its data part, in the diagram's own
/// order: each point under its parent, by position.
fn diagram_text(pkg: &Package, path: &str) -> Vec<Vec<Span>> {
    let Some(text) = pkg.text(path) else {
        return vec![];
    };
    let Ok(doc) = Xml::parse(&text) else {
        return vec![];
    };
    let mut points: Vec<(String, Node)> = Vec::new();
    let mut root = None;
    for pt in doc.descendants().filter(|n| name(n) == "pt") {
        let id = pt.attribute("modelId").unwrap_or("").to_string();
        match pt.attribute("type").unwrap_or("node") {
            "doc" => root = Some(id),
            "node" | "asst" => points.push((id, pt)),
            _ => {}
        }
    }
    let mut kids: HashMap<String, Vec<(u32, String)>> = HashMap::new();
    for c in doc.descendants().filter(|n| name(n) == "cxn") {
        if c.attribute("type").unwrap_or("parOf") != "parOf" {
            continue;
        }
        let (Some(from), Some(to)) = (c.attribute("srcId"), c.attribute("destId")) else {
            continue;
        };
        let order = c
            .attribute("srcOrd")
            .and_then(|o| o.parse().ok())
            .unwrap_or(0);
        kids.entry(from.to_string())
            .or_default()
            .push((order, to.to_string()));
    }
    let mut order: Vec<String> = Vec::new();
    let mut stack: Vec<String> = root.into_iter().collect();
    while let Some(id) = stack.pop() {
        if order.contains(&id) {
            continue;
        }
        order.push(id.clone());
        if let Some(list) = kids.get_mut(&id) {
            list.sort();
            stack.extend(list.iter().rev().map(|(_, k)| k.clone()));
        }
    }
    // Points no connection reaches keep their place in the file.
    points.sort_by_key(|(id, _)| order.iter().position(|o| o == id).unwrap_or(usize::MAX));
    points
        .iter()
        .flat_map(|(_, pt)| {
            pt.children()
                .filter(|c| name(c) == "t")
                .flat_map(|t| t.children().filter(|c| name(c) == "p"))
                .map(|p| paragraph_spans(p, &HashMap::new()))
                .collect::<Vec<_>>()
        })
        .filter(|s| s.iter().any(|x| !x.text.trim().is_empty()))
        .collect()
}

/// Every slide, and the pictures they use.
pub fn read(bytes: &[u8]) -> Result<(Vec<Slide>, Vec<Image>)> {
    let pkg = Package::open(bytes)?;
    let mut images = Vec::new();
    let slides = slide_order(&pkg)?
        .iter()
        .map(|part| read_slide(&pkg, part, &mut images))
        .collect::<Result<Vec<_>>>()?;
    Ok((slides, images))
}

/// Slides as a document: one heading per slide, optionally with speaker notes.
pub fn to_document(slides: &[Slide], images: Vec<Image>, notes: bool) -> Document {
    let mut blocks = Vec::new();
    for (i, s) in slides.iter().enumerate() {
        if i > 0 {
            blocks.push(Block::PageBreak);
        }
        let title = s
            .title
            .clone()
            .unwrap_or_else(|| format!("Slide {}", i + 1));
        blocks.push(Block::Heading(2, vec![Span::plain(title)]));
        if !s.body.is_empty() {
            blocks.push(Block::List {
                ordered: false,
                items: s
                    .body
                    .iter()
                    .map(|p| vec![Block::Paragraph(p.clone())])
                    .collect(),
            });
        }
        for table in &s.tables {
            blocks.push(Block::Table(table.clone()));
        }
        for img in &s.images {
            blocks.push(Block::Paragraph(vec![Span {
                image: Some(img.clone()),
                ..Default::default()
            }]));
        }
        if notes && let Some(n) = &s.notes {
            blocks.push(Block::Quote(vec![Block::Paragraph(vec![Span::plain(
                n.clone(),
            )])]));
        }
    }
    Document {
        blocks,
        images,
        ..Default::default()
    }
}

// ------------------------------------------------------------------ writer

const P_NS: &str = "xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const XML: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";
const WIDE: i64 = 12_192_000;
const TALL: i64 = 6_858_000;
const LEFT: i64 = 838_200;
const TITLE_TOP: i64 = 365_125;
const TITLE_HIGH: i64 = 1_325_563;
const BODY_TOP: i64 = 1_825_625;
const BODY_HIGH: i64 = 4_351_338;

/// One slide to write: a title, bullets with their depth, and pictures.
#[derive(Clone, Debug, Default)]
pub struct NewSlide {
    pub title: String,
    pub bullets: Vec<(usize, String)>,
    pub images: Vec<String>,
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn placeholder(
    id: u32,
    name: &str,
    ph: &str,
    frame: Option<(i64, i64, i64, i64)>,
    body: &str,
) -> String {
    let xfrm = frame.map_or(String::new(), |(x, y, w, h)| {
        format!("<a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{w}\" cy=\"{h}\"/></a:xfrm>")
    });
    format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"{name}\"/><p:cNvSpPr><a:spLocks noGrp=\"1\"/></p:cNvSpPr><p:nvPr><p:ph {ph}/></p:nvPr></p:nvSpPr><p:spPr>{xfrm}</p:spPr><p:txBody><a:bodyPr/><a:lstStyle/>{body}</p:txBody></p:sp>"
    )
}

const GROUP: &str = "<p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>";

fn tree(shapes: &str) -> String {
    format!("<p:cSld><p:spTree>{GROUP}{shapes}</p:spTree></p:cSld>")
}

fn master() -> String {
    let empty = "<a:p><a:endParaRPr lang=\"en-US\"/></a:p>";
    let shapes = placeholder(
        2,
        "Title",
        "type=\"title\"",
        Some((LEFT, TITLE_TOP, WIDE - 2 * LEFT, TITLE_HIGH)),
        empty,
    ) + &placeholder(
        3,
        "Body",
        "type=\"body\" idx=\"1\"",
        Some((LEFT, BODY_TOP, WIDE - 2 * LEFT, BODY_HIGH)),
        empty,
    );
    let level = |n: usize, indent: i64| {
        format!(
            "<a:lvl{n}pPr marL=\"{}\" indent=\"-228600\"><a:buChar char=\"&#8226;\"/><a:defRPr sz=\"{}\"/></a:lvl{n}pPr>",
            indent,
            2800 - 400 * (n as i64 - 1).min(3)
        )
    };
    let levels: String = (1..=5)
        .map(|n| level(n, 228_600 + 457_200 * (n as i64 - 1)))
        .collect();
    format!(
        "{XML}<p:sldMaster {P_NS}>{}<p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/><p:sldLayoutIdLst><p:sldLayoutId id=\"2147483649\" r:id=\"rId1\"/></p:sldLayoutIdLst><p:txStyles><p:titleStyle><a:lvl1pPr><a:defRPr sz=\"4400\"/></a:lvl1pPr></p:titleStyle><p:bodyStyle>{levels}</p:bodyStyle><p:otherStyle/></p:txStyles></p:sldMaster>",
        tree(&shapes).replace(
            "<p:cSld>",
            "<p:cSld><p:bg><p:bgRef idx=\"1001\"><a:schemeClr val=\"bg1\"/></p:bgRef></p:bg>"
        )
    )
}

fn layout() -> String {
    let empty = "<a:p><a:endParaRPr lang=\"en-US\"/></a:p>";
    let shapes = placeholder(2, "Title", "type=\"title\"", None, empty)
        + &placeholder(3, "Content", "idx=\"1\"", None, empty);
    format!(
        "{XML}<p:sldLayout {P_NS} type=\"obj\" preserve=\"1\">{}<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>",
        tree(&shapes).replace("<p:cSld>", "<p:cSld name=\"Title and Content\">")
    )
}

fn theme() -> String {
    let colors = [
        ("dk1", "000000"),
        ("lt1", "FFFFFF"),
        ("dk2", "44546A"),
        ("lt2", "E7E6E6"),
        ("accent1", "4472C4"),
        ("accent2", "ED7D31"),
        ("accent3", "A5A5A5"),
        ("accent4", "FFC000"),
        ("accent5", "5B9BD5"),
        ("accent6", "70AD47"),
        ("hlink", "0563C1"),
        ("folHlink", "954F72"),
    ];
    let scheme: String = colors
        .iter()
        .map(|(k, v)| format!("<a:{k}><a:srgbClr val=\"{v}\"/></a:{k}>"))
        .collect();
    let font = "<a:latin typeface=\"Calibri\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/>";
    let fill = "<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>";
    let line = format!("<a:ln w=\"9525\">{fill}</a:ln>");
    let effect = "<a:effectStyle><a:effectLst/></a:effectStyle>";
    format!(
        "{XML}<a:theme xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" name=\"Deft\"><a:themeElements><a:clrScheme name=\"Deft\">{scheme}</a:clrScheme><a:fontScheme name=\"Deft\"><a:majorFont>{font}</a:majorFont><a:minorFont>{font}</a:minorFont></a:fontScheme><a:fmtScheme name=\"Deft\"><a:fillStyleLst>{fill}{fill}{fill}</a:fillStyleLst><a:lnStyleLst>{line}{line}{line}</a:lnStyleLst><a:effectStyleLst>{effect}{effect}{effect}</a:effectStyleLst><a:bgFillStyleLst>{fill}{fill}{fill}</a:bgFillStyleLst></a:fmtScheme></a:themeElements></a:theme>"
    )
}

fn rels_xml(items: &[(String, &str, String)]) -> String {
    let body: String = items
        .iter()
        .map(|(id, kind, target)| {
            format!(
                "<Relationship Id=\"{id}\" Type=\"{REL}/{kind}\" Target=\"{}\"/>",
                esc(target)
            )
        })
        .collect();
    format!(
        "{XML}<Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{body}</Relationships>"
    )
}

/// A slide's XML and its relationships.
fn slide_xml(slide: &NewSlide, images: &[Image]) -> (String, String) {
    let title = format!(
        "<a:p><a:r><a:rPr lang=\"en-US\"/><a:t>{}</a:t></a:r></a:p>",
        esc(&slide.title)
    );
    let mut shapes = placeholder(2, "Title", "type=\"title\"", None, &title);
    let pictures: Vec<&Image> = slide
        .images
        .iter()
        .filter_map(|n| images.iter().find(|i| i.name == *n))
        .collect();
    let full = WIDE - 2 * LEFT;
    let text_width = if pictures.is_empty() || slide.bullets.is_empty() {
        full
    } else {
        full / 2
    };
    if !slide.bullets.is_empty() {
        let body: String = slide
            .bullets
            .iter()
            .map(|(level, text)| {
                format!(
                    "<a:p><a:pPr lvl=\"{}\"/><a:r><a:rPr lang=\"en-US\"/><a:t>{}</a:t></a:r></a:p>",
                    (*level).min(4),
                    esc(text)
                )
            })
            .collect();
        let frame = Some((LEFT, BODY_TOP, text_width, BODY_HIGH));
        shapes.push_str(&placeholder(3, "Content", "idx=\"1\"", frame, &body));
    }
    let mut links = vec![(
        "rId1".to_string(),
        "slideLayout",
        "../slideLayouts/slideLayout1.xml".to_string(),
    )];
    let area_x = LEFT + if text_width == full { 0 } else { text_width };
    let area_w = if text_width == full {
        full
    } else {
        full - text_width
    };
    let slot = BODY_HIGH / pictures.len().max(1) as i64;
    for (n, image) in pictures.iter().enumerate() {
        let rid = format!("rId{}", n + 2);
        links.push((rid.clone(), "image", format!("../media/{}", image.name)));
        let (w, h) = crate::docx::image_size(&image.bytes).unwrap_or((4, 3));
        let scale = (area_w as f64 / w as f64).min(slot as f64 / h as f64);
        let (cx, cy) = ((w as f64 * scale) as i64, (h as f64 * scale) as i64);
        let (x, y) = (
            area_x + (area_w - cx) / 2,
            BODY_TOP + slot * n as i64 + (slot - cy) / 2,
        );
        shapes.push_str(&format!(
            "<p:pic><p:nvPicPr><p:cNvPr id=\"{id}\" name=\"Picture {n}\"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed=\"{rid}\"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr></p:pic>",
            id = n + 10
        ));
    }
    let xml = format!(
        "{XML}<p:sld {P_NS}>{}<p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>",
        tree(&shapes)
    );
    (xml, rels_xml(&links))
}

/// Write a PowerPoint file.
pub fn write(slides: &[NewSlide], images: &[Image]) -> Result<Vec<u8>> {
    use std::io::Write as _;
    let mut types = String::from(
        "<Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"png\" ContentType=\"image/png\"/><Default Extension=\"jpg\" ContentType=\"image/jpeg\"/><Default Extension=\"jpeg\" ContentType=\"image/jpeg\"/><Default Extension=\"gif\" ContentType=\"image/gif\"/><Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/><Override PartName=\"/ppt/slideMasters/slideMaster1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml\"/><Override PartName=\"/ppt/slideLayouts/slideLayout1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml\"/><Override PartName=\"/ppt/theme/theme1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/>",
    );
    let mut ids = String::new();
    let mut links = vec![
        (
            "rId1".to_string(),
            "slideMaster",
            "slideMasters/slideMaster1.xml".to_string(),
        ),
        ("rId2".to_string(), "theme", "theme/theme1.xml".to_string()),
    ];
    let mut parts: Vec<(String, Vec<u8>)> = Vec::new();
    for (i, slide) in slides.iter().enumerate() {
        let n = i + 1;
        types.push_str(&format!("<Override PartName=\"/ppt/slides/slide{n}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>"));
        ids.push_str(&format!(
            "<p:sldId id=\"{}\" r:id=\"rId{}\"/>",
            255 + n,
            n + 2
        ));
        links.push((
            format!("rId{}", n + 2),
            "slide",
            format!("slides/slide{n}.xml"),
        ));
        let (xml, slide_rels) = slide_xml(slide, images);
        parts.push((format!("ppt/slides/slide{n}.xml"), xml.into_bytes()));
        parts.push((
            format!("ppt/slides/_rels/slide{n}.xml.rels"),
            slide_rels.into_bytes(),
        ));
    }
    let presentation = format!(
        "{XML}<p:presentation {P_NS}><p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"rId1\"/></p:sldMasterIdLst><p:sldIdLst>{ids}</p:sldIdLst><p:sldSz cx=\"{WIDE}\" cy=\"{TALL}\"/><p:notesSz cx=\"{TALL}\" cy=\"{WIDE}\"/></p:presentation>"
    );
    let mut all: Vec<(String, Vec<u8>)> = vec![
        (
            "[Content_Types].xml".into(),
            format!("{XML}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\">{types}</Types>").into_bytes(),
        ),
        (
            "_rels/.rels".into(),
            rels_xml(&[("rId1".to_string(), "officeDocument", "ppt/presentation.xml".to_string())]).into_bytes(),
        ),
        ("ppt/presentation.xml".into(), presentation.into_bytes()),
        ("ppt/_rels/presentation.xml.rels".into(), rels_xml(&links).into_bytes()),
        ("ppt/slideMasters/slideMaster1.xml".into(), master().into_bytes()),
        (
            "ppt/slideMasters/_rels/slideMaster1.xml.rels".into(),
            rels_xml(&[
                ("rId1".to_string(), "slideLayout", "../slideLayouts/slideLayout1.xml".to_string()),
                ("rId2".to_string(), "theme", "../theme/theme1.xml".to_string()),
            ])
            .into_bytes(),
        ),
        ("ppt/slideLayouts/slideLayout1.xml".into(), layout().into_bytes()),
        (
            "ppt/slideLayouts/_rels/slideLayout1.xml.rels".into(),
            rels_xml(&[("rId1".to_string(), "slideMaster", "../slideMasters/slideMaster1.xml".to_string())]).into_bytes(),
        ),
        ("ppt/theme/theme1.xml".into(), theme().into_bytes()),
    ];
    all.extend(parts);
    for image in images {
        all.push((format!("ppt/media/{}", image.name), image.bytes.clone()));
    }
    let mut zip_out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for (name, data) in all {
        zip_out
            .start_file(name, options)
            .map_err(|e| bad("pptx", e))?;
        zip_out.write_all(&data).map_err(|e| bad("pptx", e))?;
    }
    Ok(zip_out.finish().map_err(|e| bad("pptx", e))?.into_inner())
}

fn plain(spans: &[Span]) -> String {
    spans.iter().map(|s| s.text.as_str()).collect()
}

fn add_bullets(blocks: &[Block], level: usize, slide: &mut NewSlide) {
    for b in blocks {
        match b {
            Block::Paragraph(spans) => {
                let pictures: Vec<String> = spans.iter().filter_map(|s| s.image.clone()).collect();
                slide.images.extend(pictures);
                let text = plain(spans);
                if !text.trim().is_empty() {
                    slide.bullets.push((level, text));
                }
            }
            Block::List { items, .. } => {
                for item in items {
                    for inner in item {
                        let depth = level + usize::from(matches!(inner, Block::List { .. }));
                        add_bullets(std::slice::from_ref(inner), depth, slide);
                    }
                }
            }
            Block::Quote(inner) => add_bullets(inner, level, slide),
            Block::Code(code) => slide
                .bullets
                .extend(code.lines().map(|l| (level, l.to_string()))),
            Block::Table(rows) => {
                for row in rows {
                    let cells: Vec<String> = row.iter().map(|c| plain(c)).collect();
                    slide.bullets.push((level, cells.join(" | ")));
                }
            }
            _ => {}
        }
    }
}

/// Split Markdown blocks into slides: a level 1 or 2 heading or a rule starts one.
pub fn slides_from(blocks: &[Block]) -> Vec<NewSlide> {
    let mut out: Vec<NewSlide> = Vec::new();
    for b in blocks {
        match b {
            Block::Heading(level, spans) if *level <= 2 => out.push(NewSlide {
                title: plain(spans),
                ..Default::default()
            }),
            Block::Rule | Block::PageBreak => out.push(NewSlide::default()),
            Block::Heading(_, spans) => {
                if out.is_empty() {
                    out.push(NewSlide::default());
                }
                let slide = out.last_mut().expect("one slide");
                slide.bullets.push((0, plain(spans)));
            }
            other => {
                if out.is_empty() {
                    out.push(NewSlide::default());
                }
                add_bullets(
                    std::slice::from_ref(other),
                    0,
                    out.last_mut().expect("one slide"),
                );
            }
        }
    }
    out.retain(|s| !s.title.is_empty() || !s.bullets.is_empty() || !s.images.is_empty());
    out
}
