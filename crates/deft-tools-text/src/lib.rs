//! Text capabilities: Markdown to HTML (pulldown-cmark) and Markdown to PDF
//! (Markdown is translated to Typst markup and compiled in-process).

use deft_core::error::{ErrorCode, Result, err};
use deft_core::value::percent_decode;
use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use std::collections::HashMap;
use std::path::Path;
use typst::LibraryExt;
use typst::diag::{FileError, FileResult};
use typst::foundations::{Bytes, Datetime, Duration};
use typst::syntax::{FileId, RootedPath, Source, VirtualPath, VirtualRoot};
use typst::text::{Font, FontBook};
use typst::utils::LazyHash;
use typst::{Library, World};
use typst_kit::fonts::FontStore;
use typst_layout::PagedDocument;

fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_FOOTNOTES
}

/// First level-one heading (or any heading), used as the document title.
pub fn title(md: &str) -> Option<String> {
    let mut in_heading = false;
    let mut text = String::new();
    for ev in Parser::new_ext(md, options()) {
        match ev {
            Event::Start(Tag::Heading { .. }) => in_heading = true,
            Event::End(TagEnd::Heading(_)) => return Some(text).filter(|t| !t.trim().is_empty()),
            Event::Text(t) | Event::Code(t) if in_heading => text.push_str(&t),
            _ => {}
        }
    }
    None
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const CSS: &str = "body{max-width:46rem;margin:2rem auto;padding:0 1rem;font:16px/1.6 -apple-system,'Segoe UI',Roboto,'PingFang SC','Noto Sans CJK SC',sans-serif;color:#1f2328}\
pre{background:#f6f8fa;padding:.8rem;overflow:auto;border-radius:6px}code{font:.9em ui-monospace,Menlo,Consolas,monospace}\
table{border-collapse:collapse}th,td{border:1px solid #d0d7de;padding:.3rem .6rem}blockquote{margin:0;padding:0 1rem;color:#59636e;border-left:.25rem solid #d0d7de}img{max-width:100%}";

/// Render Markdown to HTML. `standalone` wraps it in a full page.
pub fn to_html(md: &str, standalone: bool, page_title: Option<&str>) -> String {
    let mut body = String::new();
    pulldown_cmark::html::push_html(&mut body, Parser::new_ext(md, options()));
    if !standalone {
        return body;
    }
    let t = page_title
        .map(str::to_string)
        .or_else(|| title(md))
        .unwrap_or_else(|| "Document".into());
    format!(
        "<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n<title>{}</title>\n<style>{CSS}</style>\n</head>\n<body>\n{body}</body>\n</html>\n",
        html_escape(&t)
    )
}

/// Escape text for Typst markup.
fn esc(text: &str) -> String {
    let mut s = String::with_capacity(text.len());
    for c in text.chars() {
        if matches!(
            c,
            '\\' | '#'
                | '*'
                | '_'
                | '`'
                | '$'
                | '<'
                | '>'
                | '@'
                | '['
                | ']'
                | '~'
                | '='
                | '-'
                | '+'
                | '/'
                | ':'
                | '\''
        ) {
            s.push('\\');
        }
        s.push(c);
    }
    s
}

/// Long unbroken text, such as a URL, gets places to break, or it runs off
/// the page. Other scripts break by themselves.
fn breakable(text: &str) -> String {
    const LIMIT: usize = 40;
    let mut out = String::with_capacity(text.len());
    let mut run = 0;
    for c in text.chars() {
        run = if c.is_ascii_graphic() { run + 1 } else { 0 };
        out.push(c);
        if run == LIMIT {
            out.push('\u{200B}');
            run = 0;
        }
    }
    out
}

/// A Typst string literal.
fn lit(text: &str) -> String {
    let mut s = String::from("\"");
    for c in text.chars() {
        match c {
            '\\' => s.push_str("\\\\"),
            '"' => s.push_str("\\\""),
            '\n' => s.push_str("\\n"),
            '\r' => {}
            '\t' => s.push_str("\\t"),
            _ => s.push(c),
        }
    }
    s.push('"');
    s
}

/// Typst markup plus the images it refers to.
struct Translation {
    markup: String,
    files: HashMap<String, Vec<u8>>,
    warnings: Vec<String>,
}

fn translate(md: &str, base: Option<&Path>) -> Translation {
    let mut out = String::new();
    let mut files = HashMap::new();
    let mut warnings = Vec::new();
    let mut code: Option<(String, String)> = None; // (language, text)
    let mut image_alt: Option<String> = None;
    let mut aligns: Vec<Alignment> = Vec::new();
    let mut in_head = false;
    for ev in Parser::new_ext(md, options()) {
        if let Some((_, text)) = code.as_mut() {
            match ev {
                Event::Text(t) => text.push_str(&t),
                Event::End(TagEnd::CodeBlock) => {
                    let (lang, text) = code.take().expect("in code block");
                    let lang = if lang.is_empty() {
                        "none".to_string()
                    } else {
                        lit(&lang)
                    };
                    out.push_str(&format!(
                        "#raw(block: true, lang: {lang}, {})\n\n",
                        lit(text.trim_end_matches('\n'))
                    ));
                }
                _ => {}
            }
            continue;
        }
        if let Some(alt) = image_alt.as_mut() {
            match ev {
                Event::Text(t) | Event::Code(t) => alt.push_str(&t),
                Event::End(TagEnd::Image) => image_alt = None,
                _ => {}
            }
            continue;
        }
        match ev {
            Event::Start(tag) => match tag {
                Tag::Heading { level, .. } => {
                    let n = match level {
                        HeadingLevel::H1 => 1,
                        HeadingLevel::H2 => 2,
                        HeadingLevel::H3 => 3,
                        HeadingLevel::H4 => 4,
                        HeadingLevel::H5 => 5,
                        HeadingLevel::H6 => 6,
                    };
                    out.push_str(&format!("#heading(level: {n})["));
                }
                Tag::Paragraph => {}
                Tag::BlockQuote(_) => out.push_str("#quote(block: true)["),
                Tag::CodeBlock(kind) => {
                    let lang = match kind {
                        CodeBlockKind::Fenced(l) => {
                            l.split_whitespace().next().unwrap_or("").to_string()
                        }
                        CodeBlockKind::Indented => String::new(),
                    };
                    code = Some((lang, String::new()));
                }
                Tag::List(Some(start)) => out.push_str(&format!("#enum(start: {start},\n")),
                Tag::List(None) => out.push_str("#list(\n"),
                Tag::Item => out.push('['),
                Tag::Emphasis => out.push_str("#emph["),
                Tag::Strong => out.push_str("#strong["),
                Tag::Strikethrough => out.push_str("#strike["),
                Tag::Link { dest_url, .. } => {
                    if dest_url.starts_with('#') {
                        out.push_str("#[");
                    } else {
                        out.push_str(&format!("#link({})[", lit(&dest_url)));
                    }
                }
                Tag::Image { dest_url, .. } => {
                    image_alt = Some(String::new());
                    let url = dest_url.to_string();
                    if url.starts_with("http://") || url.starts_with("https://") {
                        warnings.push(format!("remote image not embedded: {url}"));
                        out.push_str(&format!("#link({})[{}];", lit(&url), esc(&url)));
                        continue;
                    }
                    let located = |name: &str| {
                        base.map(|b| b.join(name))
                            .unwrap_or_else(|| Path::new(name).to_path_buf())
                    };
                    let path = located(&url);
                    // Markdown writes a space in a file name as %20.
                    let path = if path.exists() {
                        path
                    } else {
                        located(&percent_decode(&url))
                    };
                    match std::fs::read(&path) {
                        Ok(bytes) => {
                            let ext = path
                                .extension()
                                .and_then(|e| e.to_str())
                                .unwrap_or("png")
                                .to_ascii_lowercase();
                            let name = format!("img-{}.{ext}", files.len() + 1);
                            files.insert(name.clone(), bytes);
                            // No width: the natural size, shrunk to fit the column.
                            out.push_str(&format!("#box(image({}));", lit(&name)));
                        }
                        Err(_) => {
                            warnings.push(format!("image not found: {url}"));
                            out.push_str(&esc(&format!("[image: {url}]")));
                        }
                    }
                }
                Tag::Table(a) => {
                    let cols: Vec<&str> = a
                        .iter()
                        .map(|x| match x {
                            Alignment::Center => "center",
                            Alignment::Right => "right",
                            _ => "left",
                        })
                        .collect();
                    out.push_str(&format!(
                        "#table(columns: {}, align: ({},),\n",
                        a.len(),
                        cols.join(", ")
                    ));
                    aligns = a;
                }
                Tag::TableHead => {
                    in_head = true;
                    out.push_str("table.header(");
                }
                Tag::TableRow => {}
                Tag::TableCell => out.push('['),
                Tag::FootnoteDefinition(label) => {
                    out.push_str(&format!("#super[{}] ", esc(&label)))
                }
                _ => {}
            },
            Event::End(tag) => match tag {
                TagEnd::Heading(_) => out.push_str("]\n\n"),
                TagEnd::Paragraph => out.push_str("\n\n"),
                TagEnd::BlockQuote(_) => out.push_str("]\n\n"),
                TagEnd::List(_) => out.push_str(")\n\n"),
                TagEnd::Item => out.push_str("],\n"),
                // The `;` ends the call, so a following "(" or "[" stays plain text.
                TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough | TagEnd::Link => {
                    out.push_str("];")
                }
                TagEnd::Table => {
                    out.push_str(")\n\n");
                    aligns.clear();
                }
                TagEnd::TableHead => {
                    in_head = false;
                    out.push_str("),\n");
                }
                TagEnd::TableRow => out.push('\n'),
                TagEnd::TableCell => out.push_str(if in_head { "]," } else { "], " }),
                _ => {}
            },
            Event::Text(t) => out.push_str(&esc(&breakable(&t))),
            Event::Code(t) => out.push_str(&format!("#raw({});", lit(&t))),
            Event::SoftBreak => out.push(' '),
            Event::HardBreak => out.push_str(" \\\n"),
            Event::Rule => out.push_str("#line(length: 100%, stroke: 0.5pt + luma(180))\n\n"),
            Event::TaskListMarker(done) => out.push_str(if done { "☑ " } else { "☐ " }),
            Event::FootnoteReference(label) => out.push_str(&format!("#super[{}];", esc(&label))),
            Event::Html(h) if h.trim() == "<!-- page break -->" => {
                out.push_str("#pagebreak(weak: true)\n");
            }
            Event::Html(_) | Event::InlineHtml(_) => {
                if !warnings.iter().any(|w: &String| w.starts_with("raw HTML")) {
                    warnings.push("raw HTML is skipped in PDF output".to_string());
                }
            }
            _ => {}
        }
    }
    Translation {
        markup: out,
        files,
        warnings,
    }
}

/// PDF layout options.
#[derive(Clone, Debug)]
pub struct PdfStyle {
    /// Typst paper name, e.g. "a4" or "us-letter".
    pub paper: String,
    /// Margin in points.
    pub margin: f64,
    pub font_size: f64,
    pub font: Option<String>,
}

impl Default for PdfStyle {
    fn default() -> Self {
        PdfStyle {
            paper: "a4".into(),
            margin: 71.0,
            font_size: 11.0,
            font: None,
        }
    }
}

struct DocWorld {
    library: LazyHash<Library>,
    fonts: FontStore,
    main: Source,
    files: HashMap<FileId, Bytes>,
}

fn file_id(name: &str) -> FileId {
    let vpath = VirtualPath::new(name).expect("generated names are valid");
    RootedPath::new(VirtualRoot::Project, vpath).intern()
}

impl World for DocWorld {
    fn library(&self) -> &LazyHash<Library> {
        &self.library
    }
    fn book(&self) -> &LazyHash<FontBook> {
        self.fonts.book()
    }
    fn main(&self) -> FileId {
        self.main.id()
    }
    fn source(&self, id: FileId) -> FileResult<Source> {
        if id == self.main.id() {
            Ok(self.main.clone())
        } else {
            Err(FileError::AccessDenied)
        }
    }
    fn file(&self, id: FileId) -> FileResult<Bytes> {
        self.files.get(&id).cloned().ok_or(FileError::AccessDenied)
    }
    fn font(&self, index: usize) -> Option<Font> {
        self.fonts.font(index)
    }
    fn today(&self, _offset: Option<Duration>) -> Option<Datetime> {
        None
    }
}

/// Result of a PDF build.
pub struct PdfOutput {
    pub bytes: Vec<u8>,
    pub pages: usize,
    pub warnings: Vec<String>,
}

/// Whether any text fell back to the empty box of a font without the letter.
fn has_missing_glyphs(frame: &typst::layout::Frame) -> bool {
    use typst::layout::FrameItem;
    frame.items().any(|(_, item)| match item {
        FrameItem::Group(group) => has_missing_glyphs(&group.frame),
        FrameItem::Text(text) => text.glyphs.iter().any(|g| g.id == 0),
        _ => false,
    })
}

/// Compile Typst markup to PDF. System fonts are only scanned when the text
/// needs them, because scanning is slow.
fn compile(
    markup: String,
    files: HashMap<String, Vec<u8>>,
    needs_system_fonts: bool,
) -> Result<(Vec<u8>, usize, bool)> {
    let mut fonts = FontStore::new();
    for data in deft_tools_image::bundled_fonts() {
        fonts.extend(Font::iter(Bytes::new(data)).map(|font| {
            let info = font.info().clone();
            (font, info)
        }));
    }
    if needs_system_fonts {
        // macOS's LastResort claims every character and draws each as a box,
        // which would hide the real fonts behind it.
        fonts.extend(typst_kit::fonts::system().filter(|(_, info)| {
            !info
                .family
                .replace(' ', "")
                .eq_ignore_ascii_case("lastresort")
        }));
    }
    let main = Source::new(file_id("main.typ"), markup);
    let files = files
        .into_iter()
        .map(|(k, v)| (file_id(&k), Bytes::new(v)))
        .collect();
    let world = DocWorld {
        library: LazyHash::new(Library::default()),
        fonts,
        main,
        files,
    };
    let doc: PagedDocument = typst::compile(&world).output.map_err(|errors| {
        let msg: Vec<String> = errors
            .iter()
            .take(3)
            .map(|e| e.message.to_string())
            .collect();
        err(
            ErrorCode::Internal,
            format!("layout failed: {}", msg.join("; ")),
        )
    })?;
    let pages = doc.pages().len();
    let missing = doc.pages().iter().any(|p| has_missing_glyphs(&p.frame));
    let bytes = typst_pdf::pdf(&doc, &typst_pdf::PdfOptions::default()).map_err(|errors| {
        let msg: Vec<String> = errors
            .iter()
            .take(3)
            .map(|e| e.message.to_string())
            .collect();
        err(
            ErrorCode::Internal,
            format!("PDF export failed: {}", msg.join("; ")),
        )
    })?;
    Ok((bytes, pages, missing))
}

/// Render Markdown to PDF. `base` resolves relative image paths.
pub fn to_pdf(md: &str, base: Option<&Path>, style: &PdfStyle) -> Result<PdfOutput> {
    let t = translate(md, base);
    let doc_title = title(md)
        .map(|s| format!("#set document(title: {})\n", lit(&s)))
        .unwrap_or_default();
    let non_latin = md.chars().any(|c| c as u32 > 0x24F);
    let font_list = match &style.font {
        Some(f) => format!("{}, ", lit(f)),
        None => String::new(),
    };
    let preamble = format!(
        "{doc_title}#set page(paper: {paper}, margin: {margin}pt)\n\
         #set text(font: ({font_list}\"Libertinus Serif\", \"PingFang SC\", \"Noto Sans CJK SC\", \"Microsoft YaHei\"), size: {size}pt, fallback: true)\n\
         #set par(leading: 0.75em, spacing: 1.1em)\n\
         #show heading: set block(above: 1.4em, below: 0.7em)\n\
         #show link: set text(fill: rgb(\"#1a5fb4\"))\n\
         #show raw: set text(font: \"DejaVu Sans Mono\", size: 0.9em)\n\
         #show raw.where(block: true): block.with(fill: luma(245), inset: 8pt, radius: 3pt, width: 100%)\n\
         #set table(stroke: 0.5pt + luma(170), inset: 6pt)\n\n",
        paper = lit(&style.paper),
        margin = style.margin,
        size = style.font_size,
    );
    let (bytes, pages, missing) = compile(
        format!("{preamble}{}", t.markup),
        t.files,
        non_latin || style.font.is_some(),
    )?;
    let mut warnings = t.warnings;
    if missing {
        warnings.push("some characters have no font and print as empty boxes".to_string());
    }
    Ok(PdfOutput {
        bytes,
        pages,
        warnings,
    })
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;

/// One subtitle cue.
#[derive(Clone, Debug, PartialEq)]
pub struct Cue {
    pub start: f64,
    pub end: f64,
    pub text: String,
}

fn vtt_time(s: &str) -> Option<f64> {
    let s = s.trim().replace(',', ".");
    let parts: Vec<&str> = s.split(':').collect();
    let mut t = 0.0;
    for p in &parts {
        t = t * 60.0 + p.parse::<f64>().ok()?;
    }
    (parts.len() >= 2).then_some(t)
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in s.chars() {
        match c {
            '<' => depth += 1,
            '>' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
}

/// Parse WebVTT or SRT. Auto-generated captions repeat each line as they
/// scroll, so repeated lines are removed.
pub fn parse_subtitles(text: &str) -> Vec<Cue> {
    let mut cues: Vec<Cue> = Vec::new();
    let mut seen_last: Vec<String> = Vec::new();
    let text = text.replace("\r\n", "\n");
    for block in text.split("\n\n") {
        let mut lines = block.lines().peekable();
        let mut timing = None;
        for line in lines.by_ref() {
            if let Some((a, b)) = line.split_once("-->") {
                let end = b.split_whitespace().next().unwrap_or("");
                timing = vtt_time(a).zip(vtt_time(end));
                break;
            }
        }
        let Some((start, end)) = timing else { continue };
        let mut fresh = Vec::new();
        for line in lines {
            let clean = strip_tags(line).trim().to_string();
            if !clean.is_empty() && !seen_last.contains(&clean) {
                fresh.push(clean);
            }
        }
        if !fresh.is_empty() {
            seen_last = fresh.clone();
            cues.push(Cue {
                start,
                end,
                text: fresh.join(" "),
            });
        } else if end - start < 0.1 {
            continue;
        }
    }
    cues
}

/// Parse ASS/SSA dialogue. Override tags such as `{\\b1}` are dropped.
pub fn parse_ass(text: &str) -> Vec<Cue> {
    fn time(s: &str) -> Option<f64> {
        let mut t = 0.0;
        for p in s.trim().split(':') {
            t = t * 60.0 + p.parse::<f64>().ok()?;
        }
        Some(t)
    }
    let mut format: Vec<String> = vec![];
    let mut cues = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if let Some(f) = line.strip_prefix("Format:") {
            format = f.split(',').map(|s| s.trim().to_lowercase()).collect();
            continue;
        }
        let Some(body) = line.strip_prefix("Dialogue:") else {
            continue;
        };
        let col = |name: &str| format.iter().position(|f| f == name);
        let (Some(a), Some(b), Some(t)) = (col("start"), col("end"), col("text")) else {
            continue;
        };
        let parts: Vec<&str> = body.splitn(format.len(), ',').collect();
        let (Some(start), Some(end)) = (
            parts.get(a).and_then(|s| time(s)),
            parts.get(b).and_then(|s| time(s)),
        ) else {
            continue;
        };
        let mut clean = String::new();
        let mut depth = 0;
        for c in parts.get(t).copied().unwrap_or("").chars() {
            match c {
                '{' => depth += 1,
                '}' if depth > 0 => depth -= 1,
                c if depth == 0 => clean.push(c),
                _ => {}
            }
        }
        let clean = clean.replace("\\N", "\n").replace("\\n", "\n");
        if !clean.trim().is_empty() {
            cues.push(Cue {
                start,
                end,
                text: clean.trim().to_string(),
            });
        }
    }
    cues.sort_by(|x, y| x.start.total_cmp(&y.start));
    cues
}

fn clock(t: f64, sep: char) -> String {
    let ms = (t * 1000.0).round() as u64;
    format!(
        "{:02}:{:02}:{:02}{sep}{:03}",
        ms / 3_600_000,
        ms / 60_000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}

pub fn cues_to_srt(cues: &[Cue]) -> String {
    let mut s = String::new();
    for (i, c) in cues.iter().enumerate() {
        s.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            i + 1,
            clock(c.start, ','),
            clock(c.end, ','),
            c.text
        ));
    }
    s
}

pub fn cues_to_vtt(cues: &[Cue]) -> String {
    let mut s = String::from("WEBVTT\n\n");
    for c in cues {
        s.push_str(&format!(
            "{} --> {}\n{}\n\n",
            clock(c.start, '.'),
            clock(c.end, '.'),
            c.text
        ));
    }
    s
}

/// Move every cue by `seconds`. Cues pushed before zero are clipped or dropped.
pub fn shift_cues(cues: &[Cue], seconds: f64) -> Vec<Cue> {
    cues.iter()
        .filter(|c| c.end + seconds > 0.0)
        .map(|c| Cue {
            start: (c.start + seconds).max(0.0),
            end: c.end + seconds,
            text: c.text.clone(),
        })
        .collect()
}

/// Plain transcript: cues joined into paragraphs at long pauses.
pub fn cues_to_text(cues: &[Cue]) -> String {
    let mut s = String::new();
    let mut last_end = 0.0;
    for c in cues {
        if !s.is_empty() {
            s.push_str(if c.start - last_end > 2.0 {
                "\n\n"
            } else {
                " "
            });
        }
        s.push_str(&c.text);
        last_end = c.end;
    }
    s.push('\n');
    s
}

/// Markdown transcript with a timestamp at the start of each minute.
pub fn cues_to_markdown(cues: &[Cue], title: Option<&str>) -> String {
    let mut s = String::new();
    if let Some(t) = title {
        s.push_str(&format!("# {t}\n\n"));
    }
    let mut minute = -1i64;
    for c in cues {
        let m = (c.start / 60.0) as i64;
        if m != minute {
            if minute >= 0 {
                s.push_str("\n\n");
            }
            s.push_str(&format!(
                "**[{}]** ",
                deft_core::value::format_duration(c.start.floor())
            ));
            minute = m;
        } else {
            s.push(' ');
        }
        s.push_str(&c.text);
    }
    s.push('\n');
    s
}

#[cfg(test)]
#[path = "subtitle_tests.rs"]
mod subtitle_tests;
