//! Chapters of a book as well-formed, valid XHTML. Markdown can carry raw
//! HTML that browsers forgive and e-book readers refuse: named entities,
//! unclosed or unknown tags, attributes EPUB does not allow, pictures on the
//! web and links to files that are not in the book.

use crate::dom;
use html5ever::tendril::TendrilSink;
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use std::collections::{HashMap, HashSet};

/// Book chapters made safe, and what had to change.
#[derive(Debug, Default)]
pub struct Cleaned {
    /// The chapters' bodies, in the order given.
    pub bodies: Vec<String>,
    /// Pictures that are not in the book, replaced by their alt text.
    pub pictures_left_out: usize,
    /// Raw HTML tags and attributes that were dropped.
    pub markup_dropped: usize,
}

const KEEP: &[&str] = &[
    "a",
    "abbr",
    "address",
    "article",
    "aside",
    "b",
    "bdi",
    "bdo",
    "blockquote",
    "br",
    "caption",
    "cite",
    "code",
    "col",
    "colgroup",
    "data",
    "dd",
    "del",
    "details",
    "dfn",
    "div",
    "dl",
    "dt",
    "em",
    "figcaption",
    "figure",
    "footer",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "header",
    "hgroup",
    "hr",
    "i",
    "img",
    "input",
    "ins",
    "kbd",
    "li",
    "main",
    "mark",
    "nav",
    "ol",
    "p",
    "pre",
    "q",
    "rp",
    "rt",
    "ruby",
    "s",
    "samp",
    "section",
    "small",
    "span",
    "strong",
    "sub",
    "summary",
    "sup",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "time",
    "tr",
    "u",
    "ul",
    "var",
    "wbr",
];

/// Elements dropped with everything inside them.
const DROP: &[&str] = &[
    "script", "style", "template", "noscript", "iframe", "object", "embed", "source", "track",
    "param", "link", "meta", "head", "title", "base", "select", "textarea", "button",
];

const VOID: &[&str] = &["br", "col", "hr", "img", "input", "wbr"];

/// Older names with a valid equivalent.
fn renamed(tag: &str) -> Option<&'static str> {
    Some(match tag {
        "tt" => "code",
        "strike" => "s",
        "xmp" | "plaintext" => "pre",
        _ => return None,
    })
}

fn attribute_allowed(tag: &str, name: &str) -> bool {
    let global = [
        "id",
        "class",
        "title",
        "lang",
        "dir",
        "style",
        "role",
        "hidden",
        "translate",
    ];
    if global.contains(&name) || name.starts_with("aria-") || name.starts_with("data-") {
        return true;
    }
    let own: &[&str] = match tag {
        "a" => &["href", "rel", "hreflang", "type"],
        "img" => &["src", "alt", "width", "height"],
        "td" | "th" => &["colspan", "rowspan", "headers", "scope", "abbr"],
        "ol" => &["start", "reversed", "type"],
        "li" => &["value"],
        "col" | "colgroup" => &["span"],
        "blockquote" | "q" | "del" | "ins" => &["cite", "datetime"],
        "time" => &["datetime"],
        "data" => &["value"],
        "input" => &["type", "checked", "disabled"],
        "details" => &["open"],
        _ => &[],
    };
    own.contains(&name)
}

fn escape(text: &str, out: &mut String) {
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            // Characters XML 1.0 does not allow at all.
            '\u{0}'..='\u{8}'
            | '\u{B}'
            | '\u{C}'
            | '\u{E}'..='\u{1F}'
            | '\u{FFFE}'
            | '\u{FFFF}' => {}
            c => out.push(c),
        }
    }
}

fn is_xml_name(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ':'))
}

/// The parsed page and its body. The page must outlive the body: dropping a
/// node empties every node under it.
fn body_of(html: &str) -> (Handle, Handle) {
    let dom = html5ever::parse_document(RcDom::default(), Default::default()).one(html);
    let html_el = dom::children(&dom.document)
        .into_iter()
        .find(|n| dom::is(n, &["html"]));
    let body = html_el
        .and_then(|h| {
            dom::children(&h)
                .into_iter()
                .find(|n| dom::is(n, &["body"]))
        })
        .unwrap_or_else(|| dom.document.clone());
    (dom.document, body)
}

struct Writer<'a> {
    chapter: usize,
    /// Which chapter holds each id, the first one if several do.
    ids: &'a HashMap<String, usize>,
    /// File name of each chapter, for links between them.
    names: &'a [String],
    has_image: &'a dyn Fn(&str) -> bool,
    seen: HashSet<String>,
    pictures_left_out: usize,
    markup_dropped: usize,
}

impl Writer<'_> {
    fn children(&mut self, n: &Handle, out: &mut String) {
        for c in dom::children(n) {
            self.node(&c, out);
        }
    }

    fn node(&mut self, n: &Handle, out: &mut String) {
        match &n.data {
            NodeData::Text { contents } => escape(&contents.borrow(), out),
            NodeData::Element { name, attrs, .. } => {
                let ns = &*name.ns;
                if ns == "http://www.w3.org/2000/svg" || ns == "http://www.w3.org/1998/Math/MathML"
                {
                    return self.foreign(n, Some(ns), out);
                }
                let tag = name.local.to_ascii_lowercase();
                let attrs: Vec<(String, String)> = attrs
                    .borrow()
                    .iter()
                    .map(|a| (a.name.local.to_string(), a.value.to_string()))
                    .collect();
                self.element(n, &tag, &attrs, out);
            }
            _ => self.children(n, out),
        }
    }

    fn element(&mut self, n: &Handle, tag: &str, attrs: &[(String, String)], out: &mut String) {
        if DROP.contains(&tag) {
            self.markup_dropped += 1;
            return;
        }
        let get = |k: &str| attrs.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str());
        match tag {
            "img" => {
                let src = get("src").unwrap_or("");
                if !(self.has_image)(src) {
                    self.pictures_left_out += 1;
                    escape(get("alt").unwrap_or(""), out);
                    return;
                }
            }
            "a" => {
                if let Some(href) = get("href") {
                    let Some(target) = self.link(href) else {
                        return self.children(n, out);
                    };
                    let mut kept: Vec<(String, String)> =
                        attrs.iter().filter(|(k, _)| k != "href").cloned().collect();
                    kept.insert(0, ("href".to_string(), target));
                    return self.write(n, "a", &kept, out);
                }
            }
            "input" if get("type") != Some("checkbox") => {
                self.markup_dropped += 1;
                return;
            }
            _ => {}
        }
        if let Some(new) = renamed(tag) {
            return self.write(n, new, attrs, out);
        }
        if !KEEP.contains(&tag) {
            // An unknown or unsuitable element: keep what is inside it.
            self.markup_dropped += 1;
            return self.children(n, out);
        }
        self.write(n, tag, attrs, out)
    }

    /// Where a link points inside the book, or `None` when it leads nowhere.
    fn link(&self, href: &str) -> Option<String> {
        let lower = href.to_ascii_lowercase();
        if ["http:", "https:", "mailto:"]
            .iter()
            .any(|p| lower.starts_with(p))
        {
            return Some(href.to_string());
        }
        let fragment = href.strip_prefix('#')?;
        let id = deft_core::value::percent_decode(fragment);
        match self.ids.get(&id).or_else(|| self.ids.get(fragment)) {
            Some(&c) if c == self.chapter => Some(href.to_string()),
            Some(&c) => Some(format!("{}{href}", self.names[c])),
            None => None,
        }
    }

    fn write(&mut self, n: &Handle, tag: &str, attrs: &[(String, String)], out: &mut String) {
        out.push('<');
        out.push_str(tag);
        for (k, v) in attrs {
            let k = k.to_ascii_lowercase();
            let numeric = !v.is_empty() && v.chars().all(|c| c.is_ascii_digit());
            let size_ok = !matches!(k.as_str(), "width" | "height") || numeric;
            if !is_xml_name(&k) || !attribute_allowed(tag, &k) || !size_ok {
                self.markup_dropped += 1;
                continue;
            }
            if k == "id" && !self.seen.insert(v.clone()) {
                continue;
            }
            let value = if v.is_empty()
                && matches!(
                    k.as_str(),
                    "checked" | "disabled" | "open" | "reversed" | "hidden"
                ) {
                k.clone()
            } else {
                v.clone()
            };
            out.push(' ');
            out.push_str(&k);
            out.push_str("=\"");
            escape(&value, out);
            out.push('"');
        }
        if VOID.contains(&tag) {
            out.push_str("/>");
            return;
        }
        out.push('>');
        self.children(n, out);
        out.push_str("</");
        out.push_str(tag);
        out.push('>');
    }

    /// SVG and MathML keep their own names, in their own namespace.
    fn foreign(&mut self, n: &Handle, root: Option<&str>, out: &mut String) {
        let NodeData::Element { name, attrs, .. } = &n.data else {
            return self.node(n, out);
        };
        let tag = name.local.to_string();
        out.push('<');
        out.push_str(&tag);
        if let Some(ns) = root {
            out.push_str(&format!(" xmlns=\"{ns}\""));
        }
        let mut xlink = false;
        for a in attrs.borrow().iter() {
            let local = a.name.local.to_string();
            let key = match a.name.prefix.as_deref() {
                Some("xlink") => {
                    xlink = true;
                    format!("xlink:{local}")
                }
                Some(_) => continue,
                None => local,
            };
            if !is_xml_name(&key) || key.starts_with("on") || key == "xmlns" {
                continue;
            }
            out.push(' ');
            out.push_str(&key);
            out.push_str("=\"");
            escape(&a.value, out);
            out.push('"');
        }
        if xlink {
            out.push_str(" xmlns:xlink=\"http://www.w3.org/1999/xlink\"");
        }
        out.push('>');
        for c in dom::children(n) {
            match &c.data {
                NodeData::Element { .. } => self.foreign(&c, None, out),
                NodeData::Text { contents } => escape(&contents.borrow(), out),
                _ => {}
            }
        }
        out.push_str("</");
        out.push_str(&tag);
        out.push('>');
    }
}

/// Make each chapter's HTML fit for a book. `names` are the chapters' file
/// names, so that a link to a heading in another chapter can reach it, and
/// `has_image` says whether a picture is in the book.
pub fn for_book(
    chapters: &[String],
    names: &[String],
    has_image: &dyn Fn(&str) -> bool,
) -> Cleaned {
    let pages: Vec<(Handle, Handle)> = chapters.iter().map(|c| body_of(c)).collect();
    let bodies: Vec<&Handle> = pages.iter().map(|(_, body)| body).collect();
    let mut ids = HashMap::new();
    for (i, body) in bodies.iter().enumerate() {
        for e in dom::descendants(body) {
            if let Some(id) = dom::attr(&e, "id") {
                ids.entry(id).or_insert(i);
            }
        }
    }
    let mut out = Cleaned::default();
    for (i, body) in bodies.iter().enumerate() {
        let mut w = Writer {
            chapter: i,
            ids: &ids,
            names,
            has_image,
            seen: HashSet::new(),
            pictures_left_out: 0,
            markup_dropped: 0,
        };
        let mut html = String::new();
        w.children(body, &mut html);
        out.pictures_left_out += w.pictures_left_out;
        out.markup_dropped += w.markup_dropped;
        out.bodies.push(html);
    }
    out
}
