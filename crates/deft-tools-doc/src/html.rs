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
    fit_emphasis(&tree);
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
            && kids.get(i + 1).is_some_and(starts_with_link)
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

/// Whether Markdown for a node starts with a link: a link, or an inline
/// element such as `<sup>` whose first content is one, as note markers are.
fn starts_with_link(node: &Handle) -> bool {
    if dom::is(node, &["a"]) {
        return true;
    }
    if !dom::is(
        node,
        &[
            "sup", "sub", "span", "small", "b", "strong", "i", "em", "cite",
        ],
    ) {
        return false;
    }
    let first = dom::children(node).into_iter().find(|c| match &c.data {
        NodeData::Text { contents } => !contents.borrow().is_empty(),
        _ => dom::tag(c).is_some(),
    });
    first.is_some_and(|c| starts_with_link(&c))
}

/// Bold and italic that Markdown can show. A delimiter between punctuation
/// and a letter is not one, so `<b>二、</b>将` would print its asterisks:
/// punctuation at the edge of such an element moves outside it, and an
/// element of punctuation alone, such as a bold separator, loses its style.
fn fit_emphasis(node: &Handle) {
    const EMPHASIS: &[&str] = &["b", "strong", "i", "em", "s", "del", "strike"];
    for e in dom::descendants(node) {
        if !dom::is(&e, EMPHASIS)
            || dom::ancestors(&e)
                .iter()
                .any(|a| dom::is(a, &["pre", "code"]))
        {
            continue;
        }
        // <b>a</b><b>b</b> would meet as "****": one element instead.
        while let Some(parent) = dom::parent(&e) {
            let kids = dom::children(&parent);
            let Some(at) = kids.iter().position(|k| std::rc::Rc::ptr_eq(k, &e)) else {
                break;
            };
            match kids.get(at + 1) {
                Some(next) if dom::tag(next) == dom::tag(&e) => {
                    for c in dom::children(next) {
                        c.parent.set(Some(std::rc::Rc::downgrade(&e)));
                        e.children.borrow_mut().push(c);
                    }
                    next.children.borrow_mut().clear();
                    dom::detach(next);
                }
                _ => break,
            }
        }
        if dom::parent(&e).is_none() {
            continue;
        }
        let text = dom::text(&e);
        let inner =
            text.trim_matches(|c: char| c.is_whitespace() || crate::markdown::is_punctuation(c));
        if inner.is_empty() {
            if !text.trim().is_empty() {
                dom::unwrap(&e);
            }
            continue;
        }
        let Some(parent) = dom::parent(&e) else {
            continue;
        };
        let kids = dom::children(&parent);
        let Some(at) = kids.iter().position(|k| std::rc::Rc::ptr_eq(k, &e)) else {
            continue;
        };
        let wordy = |c: Option<char>| {
            c.is_some_and(|c| !c.is_whitespace() && !crate::markdown::is_punctuation(c))
        };
        // The letters beside it, past empty elements.
        let before = kids[..at]
            .iter()
            .rev()
            .find_map(|k| dom::text(k).chars().last());
        let after = kids[at + 1..]
            .iter()
            .find_map(|k| dom::text(k).chars().next());
        let body = text.trim();
        // A link, picture or code at the edge starts or ends with Markdown's
        // own punctuation, which cannot move out: such emphasis goes.
        let edge = |first: bool| {
            let kids = dom::children(&e);
            let meaningful = |c: &Handle| match &c.data {
                NodeData::Text { contents } => !contents.borrow().trim().is_empty(),
                _ => dom::tag(c).is_some(),
            };
            let pick = if first {
                kids.iter().find(|c| meaningful(c))
            } else {
                kids.iter().rev().find(|c| meaningful(c))
            };
            pick.is_some_and(|c| dom::is(c, &["a", "img", "code"]))
        };
        if (wordy(before) && edge(true)) || (wordy(after) && edge(false)) {
            dom::unwrap(&e);
            continue;
        }
        if wordy(before) && body.starts_with(crate::markdown::is_punctuation) {
            let lead: String = body
                .chars()
                .take_while(|c| crate::markdown::is_punctuation(*c))
                .collect();
            if dom::strip_text(&e, &lead, true) {
                dom::insert_text(&parent, at, &lead);
            }
        }
        if wordy(after) && body.ends_with(crate::markdown::is_punctuation) {
            let kids = dom::children(&parent);
            let at = kids
                .iter()
                .position(|k| std::rc::Rc::ptr_eq(k, &e))
                .unwrap_or(at);
            let mut trail: Vec<char> = body
                .chars()
                .rev()
                .take_while(|c| crate::markdown::is_punctuation(*c))
                .collect();
            trail.reverse();
            let trail: String = trail.into_iter().collect();
            if dom::strip_text(&e, &trail, false) {
                dom::insert_text(&parent, at + 1, &trail);
            }
        }
    }
}

/// Preformatted text is code: its content is kept as it is, in a fence.
fn pre(_: &dyn htmd::element_handler::Handlers, e: htmd::Element) -> Option<HandlerResult> {
    let code = dom::text(e.node);
    let code = code.trim_end_matches(['\n', ' ', '\t']);
    if code.trim().is_empty() {
        return Some(String::new().into());
    }
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

/// The address a saved page came from: its `<base href>`, canonical link or
/// og:url, when absolute.
pub fn page_address(html: &str) -> Option<String> {
    use deft_core::text::{attribute, unescape_attribute};
    let lower = html.to_ascii_lowercase();
    let head = &lower[..lower.find("</head").unwrap_or(lower.len())];
    let tags = |name: &str| -> Vec<std::ops::Range<usize>> {
        let open = format!("<{name}");
        let mut out = Vec::new();
        let mut from = 0;
        while let Some(at) = head[from..].find(&open) {
            let start = from + at;
            let end = head[start..]
                .find('>')
                .map_or(head.len(), |e| start + e + 1);
            out.push(start..end);
            from = end;
        }
        out
    };
    let value =
        |tag: &str, name: &str| attribute(tag, name).map(|r| unescape_attribute(tag[r].trim()));
    let absolute = |u: &String| u.starts_with("http://") || u.starts_with("https://");
    let mut found: Vec<String> = Vec::new();
    for r in tags("base") {
        found.extend(value(&html[r], "href"));
    }
    for r in tags("link") {
        let tag = &html[r];
        if value(tag, "rel").is_some_and(|v| v.eq_ignore_ascii_case("canonical")) {
            found.extend(value(tag, "href"));
        }
    }
    for r in tags("meta") {
        let tag = &html[r];
        if value(tag, "property").is_some_and(|v| v.eq_ignore_ascii_case("og:url")) {
            found.extend(value(tag, "content"));
        }
    }
    found.into_iter().find(absolute)
}

/// A link resolved against the address of the page it is on.
pub fn resolve(base: &str, link: &str) -> String {
    let scheme_end = base.find("://").map_or(0, |i| i + 3);
    let origin_end = base[scheme_end..]
        .find('/')
        .map_or(base.len(), |i| scheme_end + i);
    if link.starts_with("//") {
        return format!("{}{link}", &base[..scheme_end.saturating_sub(2)]);
    }
    let path = if link.starts_with('/') {
        link.to_string()
    } else {
        let dir_end = base[..base.find(['?', '#']).unwrap_or(base.len())]
            .rfind('/')
            .filter(|i| *i >= origin_end)
            .map_or(origin_end, |i| i + 1);
        let dir = &base[origin_end..dir_end];
        format!("{}{link}", if dir.is_empty() { "/" } else { dir })
    };
    let (path, rest) = path.split_at(path.find(['?', '#']).unwrap_or(path.len()));
    let mut parts: Vec<&str> = Vec::new();
    for seg in path.split('/') {
        match seg {
            "." => {}
            ".." => {
                if parts.len() > 1 {
                    parts.pop();
                }
            }
            s => parts.push(s),
        }
    }
    format!("{}{}{rest}", &base[..origin_end], parts.join("/"))
}

/// A saved page with its pictures made absolute where the file beside the
/// page is not there: `/static/logo.png` on a saved news page is the site's.
pub fn absolute_pictures(
    html: &str,
    folder: Option<&std::path::Path>,
    address: Option<&str>,
) -> String {
    use deft_core::text::{attribute, img_tags};
    let Some(address) = address else {
        return html.to_string();
    };
    let mut out = html.to_string();
    for tag in img_tags(html).into_iter().rev() {
        let Some(r) = attribute(&html[tag.clone()], "src") else {
            continue;
        };
        let range = tag.start + r.start..tag.start + r.end;
        let src = deft_core::text::unescape_attribute(&html[range.clone()]);
        let lower = src.trim().to_ascii_lowercase();
        if src.trim().is_empty()
            || ["http://", "https://", "//", "data:"]
                .iter()
                .any(|p| lower.starts_with(p))
        {
            continue;
        }
        let local = !src.starts_with('/')
            && folder.is_some_and(|f| {
                let path = &src[..src.find(['?', '#']).unwrap_or(src.len())];
                f.join(path).is_file() || f.join(deft_core::value::percent_decode(path)).is_file()
            });
        if !local {
            let full = resolve(address, src.trim())
                .replace('&', "&amp;")
                .replace('"', "&quot;");
            out.replace_range(range, &full);
        }
    }
    out
}
