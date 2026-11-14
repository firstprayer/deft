use crate::generated::md::_common::folder_link;
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "md.to-html",
    summary: "Render to a styled HTML page, or --fragment",
    input: Input::each(&[Format::Md, Format::Txt], "md"),
    output: Output::one(&[Format::Html], "html"),
    params: &[
        param("fragment", Kind::Flag, "Body only, no <html> page"),
        param("title", Kind::Text, "Page title (default: first heading)"),
    ],
    converts: &[Edge {
        from: &[Format::Md],
        to: &[Format::Html],
        quality: Quality::Faithful,
    }],
    examples: &[
        "deft md to-html notes.md -o notes.html",
        "deft md to-html notes.md --fragment -o -",
    ],
    tags: &["markdown to html", "render", "web page"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let input = job.input();
    let source = String::from_utf8_lossy(input.bytes()?).to_string();
    let (meta, md) = deft_tools_doc::markdown::split_front_matter(&source);
    let title = job.params.opt_text("title").or(meta.title);
    let base = input.path.as_ref().and_then(|x| x.parent());
    let missing = std::cell::RefCell::new(Vec::new());
    // Pictures are found from the Markdown file; the page may be elsewhere.
    let relink = |url: &str| -> Option<String> {
        let local = local_picture(url)?;
        let path = base.map_or_else(|| local.to_path_buf(), |b| b.join(local));
        let found = [path.clone(), percent_decoded(&path)]
            .into_iter()
            .find(|p| p.is_file());
        let Some(found) = found else {
            missing.borrow_mut().push(url.to_string());
            return None;
        };
        let link = folder_link(job, &found);
        let suffix = &url[url.find(['?', '#']).unwrap_or(url.len())..];
        Some(deft_tools_doc::markdown::link_name(&link) + suffix)
    };
    let html = deft_tools_text::to_html_relinked(
        md,
        !job.params.flag("fragment"),
        title.as_deref(),
        &relink,
    );
    let mut res = OpResult::one(Produced::bytes(html.into_bytes(), Format::Html));
    let mut missing = missing.into_inner();
    missing.dedup();
    if !missing.is_empty() {
        res = res.warn(deft_tools_text::missing_pictures_warning(&missing));
    }
    Ok(res)
}

/// The path part of a picture link to a file beside the Markdown: not a web
/// address, an absolute path or data.
fn local_picture(url: &str) -> Option<&std::path::Path> {
    let path = &url[..url.find(['?', '#']).unwrap_or(url.len())];
    let scheme = path.split_once(':').is_some_and(|(s, _)| {
        s.len() > 1
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || "+.-".contains(c))
    });
    if path.is_empty() || scheme || path.starts_with('/') || path.starts_with('\\') {
        return None;
    }
    Some(std::path::Path::new(path))
}

fn percent_decoded(path: &std::path::Path) -> std::path::PathBuf {
    std::path::PathBuf::from(deft_core::value::percent_decode(&path.to_string_lossy()))
}
