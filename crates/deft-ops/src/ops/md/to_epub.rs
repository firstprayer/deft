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
            (title, html)
        })
        .collect();
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
