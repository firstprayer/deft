//! HTML to Markdown and to plain text.

use crate::{bad, dom};
use deft_core::error::Result;
use htmd::element_handler::HandlerResult;
use markup5ever_rcdom::{Handle, NodeData};

const SKIP: &[&str] = &[
    "script", "style", "noscript", "template", "svg", "iframe", "head",
];

/// Convert a page to Markdown, dropping scripts and styles.
pub fn to_markdown(html: &str) -> Result<String> {
    let converter = htmd::HtmlToMarkdown::builder()
        .skip_tags(SKIP.to_vec())
        .add_handler(vec!["pre"], pre)
        .add_handler(vec!["a"], link)
        .build();
    let tree = converter.html_to_tree(html).map_err(|e| bad("html", e))?;
    mark_bangs(&tree);
    let text = converter.tree_to_markdown(&tree);
    Ok(tidy(&text).replace(BANG, "\\!"))
}

/// Stands for a "!" just before a link until the Markdown is written, where it
/// must be escaped or the link becomes a picture.
const BANG: char = '\u{E000}';

fn mark_bangs(node: &Handle) {
    let kids = dom::children(node);
    for (i, c) in kids.iter().enumerate() {
        if let NodeData::Text { contents } = &c.data
            && contents.borrow().ends_with('!')
            && kids.get(i + 1).is_some_and(|n| dom::is(n, &["a"]))
        {
            let mut t = contents.borrow_mut();
            let keep = t.len() - 1;
            let mut fixed = t[..keep].to_string();
            fixed.push(BANG);
            *t = fixed.into();
        }
        mark_bangs(c);
    }
}

/// Preformatted text is code: its content is kept as it is, in a fence.
fn pre(_: &dyn htmd::element_handler::Handlers, e: htmd::Element) -> Option<HandlerResult> {
    let code = dom::text(e.node);
    let code = code.trim_end_matches(['\n', ' ', '\t']);
    let language = std::iter::once(e.node.clone())
        .chain(dom::children(e.node))
        .filter_map(|n| dom::attr(&n, "class"))
        .flat_map(|c| {
            c.split_whitespace()
                .filter_map(|w| w.strip_prefix("language-").or(w.strip_prefix("lang-")))
                .map(String::from)
                .collect::<Vec<_>>()
        })
        .next()
        .unwrap_or_default();
    let longest = code.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest.max(2) + 1);
    Some(format!("\n\n{fence}{language}\n{code}\n{fence}\n\n").into())
}

const BLOCKS: &[&str] = &[
    "address",
    "article",
    "aside",
    "blockquote",
    "div",
    "dl",
    "figure",
    "footer",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hr",
    "li",
    "ol",
    "p",
    "pre",
    "section",
    "table",
    "ul",
];

/// A link around blocks, such as a card holding a heading and a paragraph,
/// cannot stay one Markdown link: the link goes on its first block instead.
fn link(handlers: &dyn htmd::element_handler::Handlers, e: htmd::Element) -> Option<HandlerResult> {
    let href = dom::attr(e.node, "href").unwrap_or_default();
    let has_blocks = dom::descendants(e.node).iter().any(|d| dom::is(d, BLOCKS));
    if !has_blocks || href.is_empty() {
        return handlers.fallback(e);
    }
    let content = handlers.walk_children(e.node).content;
    let mut parts: Vec<String> = content
        .split("\n\n")
        .map(|p| p.trim_matches('\n').to_string())
        .filter(|p| !p.trim().is_empty())
        .collect();
    let href = href
        .replace('(', "%28")
        .replace(')', "%29")
        .replace(' ', "%20");
    if let Some(first) = parts.first_mut() {
        let level = first.len() - first.trim_start_matches('#').len();
        let (marks, text) = first.split_at(level);
        let text = text.trim();
        let plain_line = !text.contains('\n') && !text.contains("](");
        if plain_line && (level == 0 || text.len() < first.len()) {
            let space = if level > 0 { " " } else { "" };
            *first = format!("{marks}{space}[{text}]({href})");
        }
    }
    Some(format!("\n\n{}\n\n", parts.join("\n\n")).into())
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
            // Inside an article, <header> holds its headline and byline.
            return without(inner, &["nav", "aside", "form"]);
        }
    }
    without(
        element(html, &lower, "body").unwrap_or(html),
        &["nav", "aside", "footer", "header", "form"],
    )
}

/// Remove these elements, with everything inside them.
fn without(html: &str, tags: &[&str]) -> String {
    let mut out = html.to_string();
    for tag in tags {
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

/// One blank line at most between blocks, no spaces at the ends of lines.
/// Code keeps its blank lines, and a line break keeps its two spaces.
fn tidy(text: &str) -> String {
    let mut out = String::new();
    let mut blank = 0;
    let mut fence: Option<String> = None;
    let lines: Vec<&str> = text.lines().collect();
    for (i, line) in lines.iter().enumerate() {
        let opener = line.trim_start();
        let ticks = opener.len() - opener.trim_start_matches('`').len();
        if ticks >= 3 {
            match &fence {
                Some(f) if opener.trim_end() == f => fence = None,
                None => fence = Some("`".repeat(ticks)),
                _ => {}
            }
        } else if fence.is_some() {
            out.push_str(line.trim_end());
            out.push('\n');
            continue;
        }
        if line.trim().is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(line.trim_end());
        let continues = lines.get(i + 1).is_some_and(|n| !n.trim().is_empty());
        if line.ends_with("  ") && !line.trim().is_empty() && continues {
            out.push_str("  ");
        }
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
