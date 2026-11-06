//! HTML to Markdown and to plain text.

use crate::bad;
use deft_core::error::Result;

const SKIP: &[&str] = &[
    "script", "style", "noscript", "template", "svg", "iframe", "head",
];

/// Convert a page to Markdown, dropping scripts and styles.
pub fn to_markdown(html: &str) -> Result<String> {
    let converter = htmd::HtmlToMarkdown::builder()
        .skip_tags(SKIP.to_vec())
        .build();
    let text = converter.convert(html).map_err(|e| bad("html", e))?;
    Ok(tidy(&text))
}

/// The inside of the first `<tag ...>...</tag>`, matching nested ones.
fn element<'a>(html: &'a str, lower: &str, tag: &str) -> Option<&'a str> {
    let open = format!("<{tag}");
    let close = format!("</{tag}");
    let start = lower.match_indices(&open).find_map(|(i, _)| {
        let next = lower.as_bytes().get(i + open.len())?;
        matches!(next, b' ' | b'>' | b'\n' | b'\t' | b'\r').then_some(i)
    })?;
    let body = start + lower[start..].find('>')? + 1;
    let (mut depth, mut at) = (1, body);
    while depth > 0 {
        let c = lower[at..].find(&close)? + at;
        depth += lower[at..c].matches(&open).count();
        depth -= 1;
        at = c + close.len();
        if depth == 0 {
            return Some(&html[body..c]);
        }
    }
    None
}

/// The article of a page: `<main>`, else `<article>`, else everything but the
/// site's navigation. Keeps boilerplate out of what an agent reads.
pub fn main_content(html: &str) -> String {
    // `to_ascii_lowercase` keeps byte offsets, so they apply to the original.
    let lower = html.to_ascii_lowercase();
    for tag in ["main", "article"] {
        if let Some(inner) = element(html, &lower, tag)
            && inner.len() > 200
        {
            return without_chrome(inner);
        }
    }
    without_chrome(element(html, &lower, "body").unwrap_or(html))
}

/// Remove navigation, headers, footers and sidebars.
fn without_chrome(html: &str) -> String {
    let mut out = html.to_string();
    for tag in ["nav", "aside", "footer", "header", "form"] {
        loop {
            let lower = out.to_ascii_lowercase();
            let Some(inner) = element(&out, &lower, tag) else {
                break;
            };
            let at = inner.as_ptr() as usize - out.as_ptr() as usize;
            let start = lower[..at].rfind(&format!("<{tag}")).unwrap_or(at);
            let close = format!("</{tag}");
            let end = at + inner.len();
            let stop = lower[end..].find('>').map_or(lower.len(), |i| end + i + 1);
            debug_assert!(lower[end..].starts_with(&close));
            out.replace_range(start..stop, "");
        }
    }
    out
}

fn tidy(text: &str) -> String {
    let mut out = String::new();
    let mut blank = 0;
    for line in text.lines() {
        if line.trim().is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out.trim().to_string() + "\n"
}

/// The readable text of a page.
pub fn to_text(html: &str) -> Result<String> {
    let md = to_markdown(html)?;
    let doc = crate::Document {
        blocks: crate::markdown::parse(&md),
        ..Default::default()
    };
    Ok(doc.text())
}

/// The content of the `<title>` element.
pub fn title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let open = lower.find("<title")?;
    let start = open + lower[open..].find('>')? + 1;
    let end = start + lower[start..].find("</title")?;
    let t = html[start..end].trim();
    (!t.is_empty()).then(|| t.to_string())
}
