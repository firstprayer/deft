use super::_common::load_images;
use deft_core::prelude::*;
use deft_tools_doc::epub::{NewBook, write};

pub static OP: Op = Op {
    id: "md.to-epub",
    summary: "Make an e-book; each top-level heading starts a chapter",
    input: Input::all(&[Format::Md, Format::Txt], 1, "md"),
    output: Output::one(&[Format::Epub], "epub"),
    params: &[
        param("title", Kind::Text, "Book title (default: first heading)"),
        param("author", Kind::Text, "Author"),
        param("lang", Kind::Text, "Language code").default("en"),
    ],
    mounts: &["epub from-md"],
    converts: &[Edge {
        from: &[Format::Md],
        to: &[Format::Epub],
        quality: Quality::Faithful,
    }],
    examples: &["deft md to-epub notes.md --author Ada -o notes.epub"],
    tags: &["ebook", "kindle", "book", "publish"],
    result: &["chapters"],
    suffix: Some("book"),
    run,
    ..Op::DEFAULT
};

/// Split a text at top-level headings, outside code fences.
fn chapters(md: &str) -> Vec<String> {
    let mut out = vec![String::new()];
    let mut fenced = false;
    for line in md.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        }
        if !fenced && line.starts_with("# ") && !out.last().is_some_and(|c| c.trim().is_empty()) {
            out.push(String::new());
        }
        let last = out.last_mut().expect("one chapter");
        last.push_str(line);
        last.push('\n');
    }
    out.retain(|c| !c.trim().is_empty());
    out
}

/// Make a chapter's HTML fit for a book. Markdown can carry raw HTML that strict
/// readers refuse: attributes EPUB does not allow, pictures on the web, and links
/// to files that are not in the book. Returns how many pictures were left out.
fn book_safe(html: &str, images: &[deft_tools_doc::Image]) -> (String, usize) {
    let re = |p: &str| regex::Regex::new(p).expect("valid pattern");
    let html = re(r#"(?i)\s(align|valign|bgcolor|border)="[^"]*""#).replace_all(html, "");
    let html = re(r#"(?i)\s(width|height)="[^"]*[^0-9"][^"]*""#).replace_all(&html, "");
    let html = re(r"(?i)</?picture\b[^>]*>|<source\b[^>]*>").replace_all(&html, "");
    let mut left_out = 0;
    let src = re(r#"(?i)\ssrc="([^"]*)""#);
    let alt = re(r#"(?i)\salt="([^"]*)""#);
    let html = re(r"(?i)<img\b[^>]*>").replace_all(&html, |c: &regex::Captures| {
        let tag = &c[0];
        let path = src
            .captures(tag)
            .map_or("", |m| m.get(1).map_or("", |g| g.as_str()));
        if images.iter().any(|i| i.name == path) {
            return tag.to_string();
        }
        left_out += 1;
        alt.captures(tag)
            .map_or(String::new(), |m| m[1].to_string())
    });
    let html = re(r#"(?is)<a\b[^>]*\bhref="([^"]*)"[^>]*>(.*?)</a>"#).replace_all(
        &html,
        |c: &regex::Captures| {
            let target = c[1].to_ascii_lowercase();
            let keep = ["http:", "https:", "mailto:", "#"]
                .iter()
                .any(|p| target.starts_with(p));
            if keep {
                c[0].to_string()
            } else {
                c[2].to_string()
            }
        },
    );
    (html.into_owned(), left_out)
}

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let mut parts = Vec::new();
    let mut images = Vec::new();
    let mut warnings = Vec::new();
    for input in &job.inputs {
        let md = String::from_utf8_lossy(input.bytes()?).to_string();
        let base = input.path.as_ref().and_then(|x| x.parent());
        let mut blocks = deft_tools_doc::markdown::parse(&md);
        let (found, problems) = load_images(&mut blocks, base);
        warnings.extend(problems);
        for image in found {
            if !images
                .iter()
                .any(|i: &deft_tools_doc::Image| i.name == image.name)
            {
                images.push(image);
            }
        }
        parts.extend(chapters(&md));
    }
    let mut left_out = 0;
    let list: Vec<(String, String)> = parts
        .iter()
        .enumerate()
        .map(|(i, md)| {
            let title = deft_tools_text::title(md).unwrap_or_else(|| format!("Chapter {}", i + 1));
            // Paths inside the book are flat, so only file names remain.
            let mut html = deft_tools_text::to_html(md, false, None);
            for image in &images {
                let quoted = format!("/{}\"", image.name);
                while let Some(at) = html.find(&quoted) {
                    let start = html[..at].rfind('"').map_or(at, |i| i + 1);
                    html.replace_range(start..at + 1, "");
                }
            }
            let (html, missed) = book_safe(&html, &images);
            left_out += missed;
            (title, html)
        })
        .collect();
    if left_out > 0 {
        warnings.push(format!(
            "{left_out} pictures left out: a book holds only local pictures"
        ));
    }
    let first = parts.first().and_then(|m| deft_tools_text::title(m));
    let title = p
        .opt_text("title")
        .or(first)
        .unwrap_or_else(|| job.input().stem());
    let author = p.opt_text("author");
    let n = list.len();
    let bytes = write(&NewBook {
        title: &title,
        author: author.as_deref(),
        language: &p.text("lang"),
        chapters: list,
        images,
    })?;
    let mut res = OpResult::one(Produced::bytes(bytes, Format::Epub))
        .fact("chapters", n)
        .note(format!("{n} chapters"));
    res.warnings = warnings;
    Ok(res)
}
