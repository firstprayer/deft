//! HTML to Markdown and to plain text.

use crate::{bad, dom};
use deft_core::error::Result;
use htmd::element_handler::HandlerResult;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};

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

/// The article of a page, without the site around it: navigation, menus,
/// language lists, share buttons, related and most-read lists, footers.
/// Takes `<main>` or the `<article>` holding most of the text, else the
/// block the paragraphs gather in, with its title and byline.
pub fn main_content(html: &str) -> String {
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
    let doc = dom.document.clone();
    let total = prose(&doc);
    let before = dom::text(&doc).trim().chars().count();
    let titles: Vec<Handle> = dom::descendants(&doc)
        .into_iter()
        .filter(|n| dom::is(n, &["h1"]))
        .collect();
    let title_texts: Vec<String> = titles
        .iter()
        .map(|t| dom::text(t).trim().to_string())
        .collect();
    prune(&doc, total, &titles);
    drop_furniture_sections(&doc, total);
    let root = choose_root(&doc);
    // A page that is all links, such as a portal's home page, has no
    // article to find: keep it whole rather than keep a scrap of it.
    if total < 500 && dom::text(&root).trim().chars().count() * 5 < before {
        return html.to_string();
    }
    let mut out = Vec::new();
    let opts = html5ever::serialize::SerializeOpts {
        traversal_scope: html5ever::serialize::TraversalScope::IncludeNode,
        ..Default::default()
    };
    // A headline set outside the article, in the page's header, goes first.
    let squash = |t: &str| -> String {
        t.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    };
    let headline =
        headline(&title_texts, html).filter(|h| !squash(&dom::text(&root)).contains(&squash(h)));
    let handle: markup5ever_rcdom::SerializableHandle = root.into();
    if html5ever::serialize(&mut out, &handle, opts).is_err() {
        return html.to_string();
    }
    let body = String::from_utf8_lossy(&out).into_owned();
    match headline {
        Some(h) => format!("<h1>{}</h1>\n{body}", escape_html(&h)),
        None => body,
    }
}

fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// The page's headline among its `<h1>` texts: the one that its og:title or
/// `<title>` holds, as "Headline | Site" does, else the first.
fn headline(h1s: &[String], html: &str) -> Option<String> {
    let squash = |t: &str| -> String {
        t.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect()
    };
    let page = squash(&og_title(html).or_else(|| title(html)).unwrap_or_default());
    let h1s: Vec<String> = h1s
        .iter()
        .map(|h| h.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|h| !h.is_empty())
        .collect();
    let matching = h1s.iter().find(|h| {
        let t = squash(h);
        !t.is_empty() && !page.is_empty() && (page.contains(&t) || t.contains(&page))
    });
    matching.or(h1s.first()).cloned()
}

/// The `og:title` a page declares for sharing.
fn og_title(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let at = lower.find("og:title")?;
    let start = lower[..at].rfind('<')?;
    let end = start + lower[start..].find('>')?;
    let tag = &html[start..end];
    let r = deft_core::text::attribute(tag, "content")?;
    let t = deft_core::text::unescape_attribute(tag[r].trim());
    (!t.is_empty()).then_some(t)
}

/// Characters of running text under a node: paragraphs and code, less
/// their links.
fn prose(node: &Handle) -> usize {
    let size = |n: &Handle| dom::text(n).trim().chars().count();
    dom::descendants(node)
        .iter()
        .filter(|d| dom::is(d, &["p", "pre"]))
        .filter(|d| !dom::ancestors(d).iter().any(|a| dom::is(a, &["p", "pre"])))
        .map(|d| {
            let links: usize = dom::descendants(d)
                .iter()
                .filter(|a| dom::is(a, &["a"]))
                .map(size)
                .sum();
            size(d).saturating_sub(links)
        })
        .sum()
}

/// Characters of text under a node, and how many are in links.
fn link_density(node: &Handle) -> f64 {
    let all = dom::text(node).trim().chars().count().max(1);
    let linked: usize = dom::descendants(node)
        .iter()
        .filter(|d| dom::is(d, &["a"]))
        .map(|a| dom::text(a).trim().chars().count())
        .sum();
    linked as f64 / all as f64
}

/// Words in class names and ids that mark site furniture.
const FURNITURE: &[&str] = &[
    "nav",
    "navbar",
    "navbox",
    "navigation",
    "menu",
    "menubar",
    "submenu",
    "breadcrumb",
    "breadcrumbs",
    "sidebar",
    "toolbar",
    "share",
    "sharing",
    "social",
    "related",
    "recommended",
    "recommendations",
    "promo",
    "advert",
    "advertisement",
    "ad",
    "ads",
    "sponsor",
    "sponsored",
    "cookie",
    "cookies",
    "consent",
    "newsletter",
    "subscribe",
    "popup",
    "modal",
    "dropdown",
    "portlet",
    "catlinks",
    "printfooter",
    "editsection",
    "noprint",
    "masthead",
    "skip",
    "jump",
    "outbrain",
    "taboola",
    "footer",
    "topbar",
    "signup",
    "paywall",
    "toc",
    "trending",
];

fn furniture_name(name: &str) -> bool {
    name.split_whitespace().any(|token| {
        let t = token.to_ascii_lowercase();
        // Wikipedia marks its "Main article:" notes so.
        if t == "navigation-not-searchable" {
            return false;
        }
        let parts: Vec<&str> = t.split(['-', '_', ':']).filter(|p| !p.is_empty()).collect();
        let hidden_in_print =
            t.contains("print") && (t.contains("none") || t.contains("hidden") || t.contains("no"));
        hidden_in_print
            || parts.iter().any(|p| FURNITURE.contains(p))
            || ["nav", "sidebar", "menu", "navigation", "toolbar", "footer"]
                .iter()
                .any(|w| t.ends_with(w) || t.starts_with(w))
    })
}

fn is_furniture(n: &Handle, holds_title: bool) -> bool {
    // An article's own footer holds its byline, date or section, not the
    // site's links: keep a short one that is mostly text.
    if dom::is(n, &["footer"])
        && dom::ancestors(n).iter().any(|a| dom::is(a, &["article"]))
        && dom::text(n).trim().chars().count() < 200
        && link_density(n) < 0.5
    {
        return false;
    }
    // Footnotes and endnotes as Sphinx and docutils write them.
    if dom::is(n, &["aside"])
        && (dom::attr(n, "class").is_some_and(|c| c.to_ascii_lowercase().contains("footnote"))
            || dom::attr(n, "role").is_some_and(|r| r.starts_with("doc-")))
    {
        return false;
    }
    const TAGS: &[&str] = &[
        "nav", "aside", "footer", "script", "style", "noscript", "template", "iframe", "form",
        "button", "select", "dialog", "menu",
    ];
    if dom::is(n, TAGS) {
        return true;
    }
    // "Skip to content" links are for screen readers on the live page.
    if dom::is(n, &["a"]) && dom::text(n).trim().to_lowercase().starts_with("skip to") {
        return true;
    }
    if dom::is(n, &["html", "body", "main", "article"]) || holds_title {
        return false;
    }
    let attr = |k: &str| dom::attr(n, k).unwrap_or_default().to_ascii_lowercase();
    let role = attr("role");
    if matches!(
        role.as_str(),
        "navigation"
            | "banner"
            | "contentinfo"
            | "complementary"
            | "search"
            | "menu"
            | "menubar"
            | "toolbar"
            | "dialog"
    ) {
        return true;
    }
    let style = attr("style").replace(' ', "");
    if attr("aria-hidden") == "true"
        || dom::attr(n, "hidden").is_some()
        || style.contains("display:none")
        || style.contains("visibility:hidden")
    {
        return true;
    }
    if dom::is(n, &["header"])
        && !dom::descendants(n)
            .iter()
            .any(|d| dom::is(d, &["h1", "h2", "h3"]))
    {
        return true;
    }
    furniture_name(&attr("class")) || furniture_name(&attr("id"))
}

/// Take furniture out of the page. Nothing holding much of the page's
/// running text goes, whatever its name says, nor a page title.
fn prune(node: &Handle, total: usize, titles: &[Handle]) {
    for c in dom::children(node) {
        if dom::tag(&c).is_none() {
            continue;
        }
        let holds_title = titles.iter().any(|h| {
            std::rc::Rc::ptr_eq(h, &c)
                || dom::ancestors(h).iter().any(|a| std::rc::Rc::ptr_eq(a, &c))
        });
        if is_furniture(&c, holds_title) && prose(&c) * 4 <= total.max(1) {
            dom::detach(&c);
        } else {
            prune(&c, total, titles);
        }
    }
}

/// Headings of lists that are about other pages.
fn furniture_heading(text: &str) -> bool {
    let t = text.trim().to_lowercase();
    const STARTS: &[&str] = &[
        "most viewed",
        "most read",
        "most popular",
        "related",
        "more from",
        "more on",
        "more stories",
        "read more",
        "recommended",
        "popular",
        "trending",
        "you may also",
        "you might also",
        "explore more",
        "in other languages",
        "languages",
        "share this",
        "hot network questions",
        "linked",
        "help improve",
        "on this page",
        "in this article",
        "contents",
        "sponsor this project",
        "contributors",
        "releases",
        "used by",
        "related content",
        "相关",
        "热门",
        "推荐",
        "أخبار مقترحة",
        "الأكثر",
        "المزيد من",
        "اقرأ أيضا",
    ];
    STARTS.iter().any(|s| t.starts_with(s))
}

/// Remove the block around a heading such as "Most viewed", as far up as
/// the block holds that heading alone.
fn drop_furniture_sections(doc: &Handle, total: usize) {
    for h in dom::descendants(doc) {
        if !dom::is(&h, &["h2", "h3", "h4"]) || !furniture_heading(&dom::text(&h)) {
            continue;
        }
        let Some(mut block) = dom::parent(&h) else {
            continue;
        };
        let headings = |n: &Handle| {
            dom::descendants(n)
                .iter()
                .filter(|d| dom::is(d, &["h1", "h2", "h3", "h4", "h5", "h6"]))
                .count()
        };
        if dom::is(&block, &["main", "article", "body", "section"]) && headings(&block) > 1 {
            // The heading sits among the article's own: take it and what
            // follows it up to the next heading.
            let kids = dom::children(&block);
            let at = kids.iter().position(|k| std::rc::Rc::ptr_eq(k, &h));
            if let Some(at) = at {
                let mut gone = vec![kids[at].clone()];
                for k in &kids[at + 1..] {
                    if dom::is(k, &["h1", "h2", "h3", "h4"]) {
                        break;
                    }
                    gone.push(k.clone());
                }
                let size: usize = gone.iter().map(prose).sum();
                if link_density_all(&gone) > 0.5 && size * 4 <= total.max(1) {
                    gone.iter().for_each(dom::detach);
                }
            }
            continue;
        }
        while let Some(up) = dom::parent(&block) {
            // Not past the largest block that is still small: one more step
            // could take in the article.
            if dom::is(&up, &["main", "article", "body", "html"])
                || headings(&up) > 1
                || prose(&up) * 4 > total.max(1)
            {
                break;
            }
            block = up;
        }
        if prose(&block) * 4 <= total.max(1) {
            dom::detach(&block);
        }
    }
}

fn link_density_all(nodes: &[Handle]) -> f64 {
    let all: usize = nodes
        .iter()
        .map(|n| dom::text(n).trim().chars().count())
        .sum();
    let linked: usize = nodes
        .iter()
        .flat_map(|n| {
            let mut v = dom::descendants(n);
            v.push(n.clone());
            v
        })
        .filter(|d| dom::is(d, &["a"]))
        .map(|a| dom::text(&a).trim().chars().count())
        .sum();
    linked as f64 / all.max(1) as f64
}

/// The element holding the article.
fn choose_root(doc: &Handle) -> Handle {
    let all = dom::descendants(doc);
    let body = all
        .iter()
        .find(|n| dom::is(n, &["body"]))
        .cloned()
        .unwrap_or_else(|| doc.clone());
    let main = all
        .iter()
        .find(|n| dom::is(n, &["main"]) || dom::attr(n, "role").as_deref() == Some("main"))
        .cloned();
    // A page with little running text, such as a home page, is all content.
    let mut root = match main {
        Some(m) => m,
        None if prose(&body) < 500 => return body,
        None => best_block(&body).unwrap_or_else(|| body.clone()),
    };
    let top = root.clone();
    // An article holding most of the text is the article.
    let size = prose(&root);
    if let Some(article) = dom::descendants(&root)
        .into_iter()
        .filter(|n| dom::is(n, &["article"]))
        .max_by_key(prose)
        && prose(&article) * 2 >= size
        && size > 0
    {
        root = article;
    }
    // Take in the title and byline beside the text, but not the main
    // element's other content, nor what lies outside it.
    let text = |n: &Handle| dom::text(n).trim().chars().count();
    while !std::rc::Rc::ptr_eq(&root, &top) || !dom::is(&top, &["main"]) {
        let Some(up) = dom::parent(&root) else { break };
        if dom::is(&up, &["html", "body"]) || dom::tag(&up).is_none() {
            break;
        }
        let extra = text(&up).saturating_sub(text(&root));
        let title_outside = dom::descendants(&up).iter().any(|h| {
            dom::is(h, &["h1", "h2"])
                && !dom::ancestors(h)
                    .iter()
                    .any(|a| std::rc::Rc::ptr_eq(a, &root))
                && !std::rc::Rc::ptr_eq(h, &root)
        });
        // A root that holds its headline needs no more; what a long article
        // may take in around it is capped, or it climbs into the page.
        let has_h1 = dom::descendants(&root).iter().any(|d| dom::is(d, &["h1"]));
        let limit = (text(&root) / 10).min(400);
        if extra > limit && !(title_outside && !has_h1 && extra * 10 <= text(&root) * 3) {
            break;
        }
        let at_main = dom::is(&up, &["main"]);
        root = up;
        if at_main {
            break;
        }
    }
    root
}

/// The block the paragraphs gather in, scored as Readability does: each
/// paragraph counts for its parent, and half for its grandparent.
fn best_block(body: &Handle) -> Option<Handle> {
    let mut scores: Vec<(Handle, f64)> = Vec::new();
    let mut add =
        |n: Handle, s: f64| match scores.iter_mut().find(|(k, _)| std::rc::Rc::ptr_eq(k, &n)) {
            Some(entry) => entry.1 += s,
            None => scores.push((n, s)),
        };
    for p in dom::descendants(body)
        .into_iter()
        .filter(|d| dom::is(d, &["p", "pre"]))
    {
        let text = dom::text(&p);
        let len = text.trim().chars().count();
        if len < 25 {
            continue;
        }
        let score = 1.0
            + text.matches([',', '，', '、', '،']).count() as f64
            + (len as f64 / 100.0).min(3.0);
        if let Some(parent) = dom::parent(&p) {
            if let Some(grand) = dom::parent(&parent) {
                add(grand, score / 2.0);
            }
            add(parent, score);
        }
    }
    scores
        .into_iter()
        .map(|(n, s)| {
            let d = link_density(&n);
            (n, s * (1.0 - d))
        })
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(n, _)| n)
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
