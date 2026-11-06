//! EPUB books: read chapters in spine order, write a book from Markdown.

use crate::zipped::{Package, join};
use crate::{Image, bad, html};
use deft_core::error::Result;
use roxmltree::Document as Xml;
use std::collections::HashMap;
use std::io::Write;

#[derive(Clone, Debug, Default)]
pub struct Book {
    pub title: Option<String>,
    pub author: Option<String>,
    pub language: Option<String>,
    /// Chapters as HTML, in reading order.
    pub chapters: Vec<String>,
    pub images: Vec<Image>,
}

fn opf_path(pkg: &Package) -> Result<String> {
    let text = pkg
        .text("META-INF/container.xml")
        .ok_or_else(|| bad("epub", "no META-INF/container.xml"))?;
    let doc = Xml::parse(&text).map_err(|e| bad("epub", e))?;
    doc.descendants()
        .find(|n| n.tag_name().name() == "rootfile")
        .and_then(|n| n.attribute("full-path"))
        .map(String::from)
        .ok_or_else(|| bad("epub", "no rootfile"))
}

pub fn read(bytes: &[u8]) -> Result<Book> {
    let pkg = Package::open(bytes)?;
    let opf = opf_path(&pkg)?;
    let text = pkg.text(&opf).ok_or_else(|| bad("epub", &opf))?;
    let doc = Xml::parse(&text).map_err(|e| bad("epub", e))?;
    let meta = |name: &str| {
        doc.descendants()
            .find(|n| n.tag_name().name() == name)
            .and_then(|n| n.text())
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
    };
    let mut manifest = HashMap::new();
    let mut images = Vec::new();
    for item in doc.descendants().filter(|n| n.tag_name().name() == "item") {
        let (Some(id), Some(href)) = (item.attribute("id"), item.attribute("href")) else {
            continue;
        };
        let path = join(&opf, href);
        if item
            .attribute("media-type")
            .is_some_and(|t| t.starts_with("image/"))
            && let Some(bytes) = pkg.read(&path)
        {
            let name = href.rsplit('/').next().unwrap_or(href).to_string();
            images.push(Image { name, bytes });
        }
        manifest.insert(id.to_string(), path);
    }
    let mut chapters = Vec::new();
    for r in doc
        .descendants()
        .filter(|n| n.tag_name().name() == "itemref")
    {
        if r.attribute("linear") == Some("no") {
            continue;
        }
        if let Some(html) = r
            .attribute("idref")
            .and_then(|id| manifest.get(id))
            .and_then(|path| pkg.text(path))
        {
            chapters.push(html);
        }
    }
    Ok(Book {
        title: meta("title"),
        author: meta("creator"),
        language: meta("language"),
        chapters,
        images,
    })
}

impl Book {
    /// The whole book as Markdown.
    pub fn markdown(&self) -> Result<String> {
        let mut out = String::new();
        for c in &self.chapters {
            let md = html::to_markdown(c)?;
            if !md.trim().is_empty() {
                out.push_str(&md);
                out.push('\n');
            }
        }
        // Images sit in folders inside the book; keep only their file names.
        let re = regex::Regex::new(r"!\[([^\]]*)\]\((?:[^)/]*/)*([^)/]+)\)")
            .map_err(|e| bad("regex", e))?;
        Ok(re.replace_all(&out, "![$1]($2)").to_string())
    }
}

fn xml_text(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// What `write` needs. Chapters are (title, XHTML body).
pub struct NewBook<'a> {
    pub title: &'a str,
    pub author: Option<&'a str>,
    pub language: &'a str,
    pub chapters: Vec<(String, String)>,
    pub images: Vec<Image>,
}

fn media_type(name: &str) -> &'static str {
    match name
        .rsplit('.')
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
        .as_str()
    {
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "webp" => "image/webp",
        _ => "image/png",
    }
}

/// Write an EPUB 3 file, stable from one run to the next.
pub fn write(book: &NewBook) -> Result<Vec<u8>> {
    let mut manifest = String::new();
    let mut spine = String::new();
    let mut nav = String::new();
    let mut files: Vec<(String, Vec<u8>)> = Vec::new();
    for (i, (title, body)) in book.chapters.iter().enumerate() {
        let name = format!("chapter-{:03}.xhtml", i + 1);
        manifest.push_str(&format!(
            "<item id=\"c{i}\" href=\"{name}\" media-type=\"application/xhtml+xml\"/>\n"
        ));
        spine.push_str(&format!("<itemref idref=\"c{i}\"/>\n"));
        nav.push_str(&format!(
            "<li><a href=\"{name}\">{}</a></li>\n",
            xml_text(title)
        ));
        let page = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xml:lang=\"{lang}\"><head><title>{t}</title></head><body>\n{body}</body></html>\n",
            lang = xml_text(book.language),
            t = xml_text(title),
        );
        files.push((format!("OEBPS/{name}"), page.into_bytes()));
    }
    for (i, img) in book.images.iter().enumerate() {
        manifest.push_str(&format!(
            "<item id=\"i{i}\" href=\"{}\" media-type=\"{}\"/>\n",
            xml_text(&img.name),
            media_type(&img.name)
        ));
        files.push((format!("OEBPS/{}", img.name), img.bytes.clone()));
    }
    let id: u64 = book
        .chapters
        .iter()
        .fold(14695981039346656037, |h, (_, b)| {
            b.bytes()
                .fold(h, |h, x| (h ^ x as u64).wrapping_mul(1099511628211))
        });
    let creator = book
        .author
        .map(|a| format!("<dc:creator>{}</dc:creator>\n", xml_text(a)))
        .unwrap_or_default();
    let opf = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<package xmlns=\"http://www.idpf.org/2007/opf\" version=\"3.0\" unique-identifier=\"id\">\n<metadata xmlns:dc=\"http://purl.org/dc/elements/1.1/\">\n<dc:identifier id=\"id\">urn:deft:{id:016x}</dc:identifier>\n<dc:title>{title}</dc:title>\n{creator}<dc:language>{lang}</dc:language>\n<meta property=\"dcterms:modified\">2000-01-01T00:00:00Z</meta>\n</metadata>\n<manifest>\n<item id=\"nav\" href=\"nav.xhtml\" media-type=\"application/xhtml+xml\" properties=\"nav\"/>\n{manifest}</manifest>\n<spine>\n{spine}</spine>\n</package>\n",
        title = xml_text(book.title),
        lang = xml_text(book.language),
    );
    let nav_page = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>{t}</title></head><body><nav epub:type=\"toc\"><ol>\n{nav}</ol></nav></body></html>\n",
        t = xml_text(book.title),
    );
    let mut zip_out = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let stored =
        zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    zip_out
        .start_file("mimetype", stored)
        .map_err(|e| bad("epub", e))?;
    zip_out
        .write_all(b"application/epub+zip")
        .map_err(|e| bad("epub", e))?;
    let container = "<?xml version=\"1.0\"?>\n<container version=\"1.0\" xmlns=\"urn:oasis:names:tc:opendocument:xmlns:container\"><rootfiles><rootfile full-path=\"OEBPS/content.opf\" media-type=\"application/oebps-package+xml\"/></rootfiles></container>\n";
    let mut all: Vec<(String, Vec<u8>)> = vec![
        (
            "META-INF/container.xml".into(),
            container.as_bytes().to_vec(),
        ),
        ("OEBPS/content.opf".into(), opf.into_bytes()),
        ("OEBPS/nav.xhtml".into(), nav_page.into_bytes()),
    ];
    all.extend(files);
    for (name, data) in all {
        zip_out
            .start_file(name, deflated)
            .map_err(|e| bad("epub", e))?;
        zip_out.write_all(&data).map_err(|e| bad("epub", e))?;
    }
    Ok(zip_out.finish().map_err(|e| bad("epub", e))?.into_inner())
}
