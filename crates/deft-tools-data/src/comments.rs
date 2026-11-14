//! Comments of YAML and TOML documents, kept beside the value model so that a
//! conversion between the two can write them back next to the same keys.

use std::collections::HashMap;

/// One step into a document: an object key or a list position.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Seg {
    Key(String),
    Index(usize),
}

pub type Path = Vec<Seg>;

/// The comments that belong to one node.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Note {
    /// Whole-line comments just above the node, without the `#`.
    pub before: Vec<String>,
    /// A comment at the end of the node's first line.
    pub after: Option<String>,
    /// Whole-line comments at the end of a table or list, after its last entry.
    pub end: Vec<String>,
    /// A blank line separated the node from what came before.
    pub blank: bool,
    /// Where the first comment stood in the source, to keep their order.
    pub order: usize,
}

/// Every comment of a document, by the path of the node it belongs to.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Comments {
    pub at: HashMap<Path, Note>,
}

impl Comments {
    pub fn is_empty(&self) -> bool {
        self.at
            .values()
            .all(|n| n.before.is_empty() && n.after.is_none() && n.end.is_empty() && !n.blank)
    }

    pub fn count(&self) -> usize {
        self.at
            .values()
            .map(|n| n.before.len() + n.end.len() + usize::from(n.after.is_some()))
            .sum()
    }

    fn note(&mut self, path: &[Seg], order: usize) -> &mut Note {
        let n = self.at.entry(path.to_vec()).or_default();
        if n.before.is_empty() && n.after.is_none() && n.end.is_empty() {
            n.order = order;
        }
        n
    }

    pub fn get(&self, path: &[Seg]) -> Option<&Note> {
        self.at.get(path)
    }

    /// Notes below `path` (not at it), in source order.
    pub fn below(&self, path: &[Seg]) -> Vec<(&Path, &Note)> {
        let mut v: Vec<_> = self
            .at
            .iter()
            .filter(|(p, _)| p.len() > path.len() && p.starts_with(path))
            .collect();
        v.sort_by_key(|(p, n)| (n.order, (*p).clone()));
        v
    }

    /// Move the comments of nodes the value does not have (a dropped null, a
    /// YAML merge key) to the end of the nearest container that it does have.
    pub fn rehome(&mut self, value: &serde_json::Value) {
        let lost: Vec<Path> = self
            .at
            .keys()
            .filter(|p| !exists(value, p))
            .cloned()
            .collect();
        let mut lost: Vec<(Path, Note)> = lost
            .into_iter()
            .filter_map(|p| self.at.remove(&p).map(|n| (p, n)))
            .collect();
        lost.sort_by_key(|(p, n)| (n.order, p.clone()));
        for (mut p, n) in lost {
            while !p.is_empty() && !exists(value, &p) {
                p.pop();
            }
            // A scalar cannot hold comments after it; its parent can.
            if !p.is_empty() && !lookup(value, &p).is_some_and(|v| v.is_object() || v.is_array()) {
                p.pop();
            }
            let order = n.order;
            let home = self.note(&p, order);
            home.end.extend(n.before);
            home.end.extend(n.after);
            home.end.extend(n.end);
        }
    }
}

pub fn lookup<'a>(v: &'a serde_json::Value, path: &[Seg]) -> Option<&'a serde_json::Value> {
    path.iter().try_fold(v, |v, s| match s {
        Seg::Key(k) => v.as_object()?.get(k),
        Seg::Index(i) => v.as_array()?.get(*i),
    })
}

pub fn lookup_mut<'a>(
    v: &'a mut serde_json::Value,
    path: &[Seg],
) -> Option<&'a mut serde_json::Value> {
    path.iter().try_fold(v, |v, s| match s {
        Seg::Key(k) => v.as_object_mut()?.get_mut(k),
        Seg::Index(i) => v.as_array_mut()?.get_mut(*i),
    })
}

fn exists(v: &serde_json::Value, path: &[Seg]) -> bool {
    lookup(v, path).is_some()
}

/// The text of a comment as it is written back: control characters are not
/// allowed in a TOML comment.
pub fn clean(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() && c != '\t' { ' ' } else { c })
        .collect()
}

// ---------------------------------------------------------------- YAML

/// A comment found in YAML text.
#[derive(Debug, Clone)]
struct Raw {
    line: usize,
    text: String,
    whole_line: bool,
    blank_before: bool,
}

fn indent_of(line: &str) -> usize {
    line.len() - line.trim_start_matches(' ').len()
}

/// The comments of YAML text, line by line. Quoted text and block scalars
/// (`|` and `>`) are skipped, since a `#` inside them is content.
fn yaml_raw_comments(text: &str) -> Vec<Raw> {
    let mut out = Vec::new();
    let mut quote: Option<char> = None;
    // Lines more indented than this belong to a block scalar.
    let mut block: Option<usize> = None;
    let mut blank = false;
    for (n, line) in text.lines().enumerate() {
        if let Some(parent) = block {
            if line.trim().is_empty() || indent_of(line) > parent {
                continue;
            }
            block = None;
        }
        if quote.is_none() && line.trim().is_empty() {
            blank = true;
            continue;
        }
        let chars: Vec<(usize, char)> = line.char_indices().collect();
        let mut comment_at = None;
        let mut i = 0;
        let mut prev_nonspace: Option<char> = None;
        while i < chars.len() {
            let (at, c) = chars[i];
            match quote {
                Some('"') => match c {
                    '\\' => i += 1,
                    '"' => quote = None,
                    _ => {}
                },
                Some(_) => {
                    if c == '\'' {
                        if chars.get(i + 1).is_some_and(|x| x.1 == '\'') {
                            i += 1;
                        } else {
                            quote = None;
                        }
                    }
                }
                None => {
                    let after_space = i == 0 || chars[i - 1].1.is_whitespace();
                    if c == '#' && after_space {
                        comment_at = Some(at);
                        break;
                    }
                    let starts_scalar = prev_nonspace.is_none_or(|p| ":-[{,?".contains(p));
                    if (c == '"' || c == '\'') && starts_scalar && after_space_or_flow(&chars, i) {
                        quote = Some(c);
                    }
                }
            }
            if !c.is_whitespace() {
                prev_nonspace = Some(c);
            }
            i += 1;
        }
        let content = &line[..comment_at.unwrap_or(line.len())];
        if let Some(at) = comment_at {
            out.push(Raw {
                line: n,
                text: line[at + 1..].trim_end().to_string(),
                whole_line: content.trim().is_empty(),
                blank_before: blank && content.trim().is_empty(),
            });
        }
        if !content.trim().is_empty() {
            blank = false;
        }
        if quote.is_none() && starts_block(content) {
            block = Some(block_parent(content));
        }
    }
    out
}

fn after_space_or_flow(chars: &[(usize, char)], i: usize) -> bool {
    i == 0 || {
        let p = chars[i - 1].1;
        p.is_whitespace() || "[{,".contains(p)
    }
}

/// True when a line ends with a block scalar indicator such as `|`, `>-` or `|2`.
fn starts_block(content: &str) -> bool {
    let t = content.trim_end();
    let Some(pos) = t.rfind(['|', '>']) else {
        return false;
    };
    let tail = &t[pos + 1..];
    let before = &t[..pos];
    tail.len() <= 2
        && tail
            .chars()
            .all(|c| c == '+' || c == '-' || c.is_ascii_digit())
        && (before.is_empty() || before.ends_with(' '))
        && {
            let b = before.trim_end();
            b.is_empty() || b.ends_with(':') || b.ends_with('-') || b.contains(": ") || {
                // `key: !!str |` or `key: &anchor |`
                b.rsplit(' ')
                    .next()
                    .is_some_and(|w| w.starts_with('!') || w.starts_with('&'))
            }
        }
}

/// Lines indented past this column belong to the block scalar a line opens.
fn block_parent(content: &str) -> usize {
    let mut col = indent_of(content);
    let mut rest = &content[col..];
    let mut dash = None;
    while let Some(r) = rest.strip_prefix('-') {
        if !(r.is_empty() || r.starts_with(' ')) {
            break;
        }
        dash = Some(col);
        let skip = 1 + (r.len() - r.trim_start_matches(' ').len());
        col += skip;
        rest = &rest[skip..];
    }
    if rest.starts_with(['|', '>']) {
        return dash.unwrap_or(col);
    }
    col
}

enum Frame {
    Map { path: Path, key: Option<String> },
    Seq { path: Path, next: usize },
}

struct Walker {
    anchors: Vec<(usize, Path)>,
    stack: Vec<Frame>,
    doc: usize,
    /// Values with a `!!` tag outside YAML's own types, such as
    /// `!!python/name:module.fn`, which serde_yaml drops.
    tags: Vec<(Path, String)>,
}

/// `!!` tags written in YAML text outside comments, such as one on a
/// mapping (`!!python/object/apply:f`), which no event reports.
pub fn yaml_text_tags(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let code = match line.find(" #") {
            Some(i) => &line[..i],
            None if line.trim_start().starts_with('#') => "",
            None => line,
        };
        for word in code.split([' ', '[', '{', ',']) {
            if let Some(tag) = word.strip_prefix("!!") {
                let name = tag.split(':').next().unwrap_or("");
                if !tag.is_empty() && !CORE_TAGS.contains(&name) {
                    out.push(format!("!!{tag}"));
                }
            }
        }
    }
    out
}

/// The `!!` tags YAML defines itself.
const CORE_TAGS: &[&str] = &[
    "str",
    "int",
    "float",
    "bool",
    "null",
    "map",
    "seq",
    "binary",
    "timestamp",
    "set",
    "omap",
    "pairs",
    "merge",
    "value",
];

impl Walker {
    /// The path of a node that starts now; `None` for a mapping key, which is
    /// recorded as an anchor instead.
    fn node(&mut self, line: usize, key_text: Option<&str>) -> Option<Path> {
        let doc = Seg::Index(self.doc);
        match self.stack.last_mut() {
            None => Some(vec![doc]),
            Some(Frame::Seq { path, next }) => {
                let mut p = path.clone();
                p.push(Seg::Index(*next));
                *next += 1;
                self.anchors.push((line, p.clone()));
                Some(p)
            }
            Some(Frame::Map { path, key }) => match key.take() {
                None => {
                    let k = key_text.unwrap_or("").to_string();
                    let mut p = path.clone();
                    p.push(Seg::Key(k.clone()));
                    self.anchors.push((line, p));
                    *key = Some(k);
                    None
                }
                Some(k) => {
                    let mut p = path.clone();
                    p.push(Seg::Key(k));
                    Some(p)
                }
            },
        }
    }
}

impl yaml_rust::parser::MarkedEventReceiver for Walker {
    fn on_event(&mut self, ev: yaml_rust::Event, mark: yaml_rust::scanner::Marker) {
        use yaml_rust::Event as E;
        let line = mark.line().saturating_sub(1);
        match ev {
            E::DocumentStart => self.stack.clear(),
            E::DocumentEnd => self.doc += 1,
            E::Scalar(s, _, _, tag) => {
                let path = self.node(line, Some(&s));
                if let (Some(path), Some(yaml_rust::scanner::TokenType::Tag(handle, suffix))) =
                    (path, tag)
                    && handle == "!!"
                    && !CORE_TAGS.contains(&suffix.as_str())
                {
                    self.tags.push((path, format!("!!{suffix}")));
                }
            }
            E::Alias(_) => {
                self.node(line, Some("<<alias>>"));
            }
            E::MappingStart(_) => {
                if let Some(path) = self.node(line, None) {
                    self.stack.push(Frame::Map { path, key: None });
                } else {
                    // A complex key: give it a placeholder frame.
                    self.stack.push(Frame::Map {
                        path: vec![],
                        key: None,
                    });
                }
            }
            E::SequenceStart(_) => {
                let path = self.node(line, None).unwrap_or_default();
                self.stack.push(Frame::Seq { path, next: 0 });
            }
            E::MappingEnd | E::SequenceEnd => {
                self.stack.pop();
            }
            _ => {}
        }
    }
}

/// What a YAML text holds beside its data: its comments, each tied to the
/// key or item it describes (a whole-line comment to the node below it, an
/// end-of-line comment to the node on its line), and the `!!` tags serde_yaml
/// drops. The comments are `None` when the text cannot be read this way.
pub fn yaml_extras(text: &str, documents: usize) -> (Option<Comments>, Vec<(Path, String)>) {
    let raws = yaml_raw_comments(text);
    if raws.is_empty() && !text.contains("!!") {
        return (Some(Comments::default()), Vec::new());
    }
    let mut w = Walker {
        anchors: Vec::new(),
        stack: Vec::new(),
        doc: 0,
        tags: Vec::new(),
    };
    let mut parser = yaml_rust::parser::Parser::new(text.chars());
    if parser.load(&mut w, true).is_err() {
        return (raws.is_empty().then(Comments::default), Vec::new());
    }
    // One document is the value itself, several are a list of documents.
    let strip = |mut p: Path| {
        if documents <= 1 && !p.is_empty() {
            p.remove(0);
        }
        p
    };
    let tags = w.tags.into_iter().map(|(p, t)| (strip(p), t)).collect();
    let mut anchors: Vec<(usize, Path)> =
        w.anchors.into_iter().map(|(l, p)| (l, strip(p))).collect();
    anchors.sort_by_key(|a| a.0);
    let mut c = Comments::default();
    let root: Path = Vec::new();
    for (order, r) in raws.into_iter().enumerate() {
        if r.whole_line {
            match anchors.iter().find(|a| a.0 > r.line) {
                Some((_, p)) => {
                    let n = c.note(p, order);
                    n.before.push(r.text);
                    n.blank |= r.blank_before && n.before.len() == 1;
                }
                None => c.note(&root, order).end.push(r.text),
            }
        } else {
            // The deepest node that starts on this line, else the last before it.
            let on_line = anchors.iter().rev().find(|a| a.0 == r.line);
            let target = on_line.or_else(|| anchors.iter().rev().find(|a| a.0 < r.line));
            match target {
                Some((_, p)) => {
                    let n = c.note(p, order);
                    match &mut n.after {
                        None => n.after = Some(r.text),
                        Some(_) => n.end.push(r.text),
                    }
                }
                None => c.note(&root, order).before.push(r.text),
            }
        }
    }
    (Some(c), tags)
}

// ---------------------------------------------------------------- TOML

fn decor_lines(raw: Option<&str>) -> (Vec<String>, bool) {
    let mut out = Vec::new();
    let mut blank = false;
    let Some(raw) = raw else {
        return (out, blank);
    };
    for line in raw.split('\n') {
        let t = line.trim();
        if let Some(c) = t.strip_prefix('#') {
            out.push(c.trim_end().to_string());
        } else if t.is_empty() && out.is_empty() && raw.contains("\n\n") {
            blank = true;
        }
    }
    (out, blank)
}

fn suffix_comment(raw: Option<&str>) -> Option<String> {
    let t = raw?.trim();
    t.strip_prefix('#').map(|c| c.trim_end().to_string())
}

struct TomlWalk {
    c: Comments,
    order: usize,
}

impl TomlWalk {
    fn before(&mut self, path: &[Seg], raw: Option<&str>) {
        let (lines, blank) = decor_lines(raw);
        if lines.is_empty() && !blank {
            return;
        }
        self.order += 1;
        let n = self.c.note(path, self.order);
        n.blank |= blank;
        n.before.extend(lines);
    }

    fn after(&mut self, path: &[Seg], raw: Option<&str>) {
        if let Some(t) = suffix_comment(raw) {
            self.order += 1;
            let n = self.c.note(path, self.order);
            match n.after {
                None => n.after = Some(t),
                Some(_) => n.end.push(t),
            }
        }
    }

    fn end(&mut self, path: &[Seg], lines: Vec<String>) {
        if !lines.is_empty() {
            self.order += 1;
            self.c.note(path, self.order).end.extend(lines);
        }
    }

    fn table(&mut self, t: &toml_edit::Table, path: &Path) {
        for (k, item) in t.iter() {
            let mut p = path.clone();
            p.push(Seg::Key(k.to_string()));
            let key = t.key(k);
            match item {
                toml_edit::Item::Value(v) => {
                    self.before(&p, key.and_then(|k| k.leaf_decor().prefix()?.as_str()));
                    self.value(v, &p);
                }
                toml_edit::Item::Table(sub) => {
                    if sub.is_dotted() {
                        self.before(&p, key.and_then(|k| k.leaf_decor().prefix()?.as_str()));
                    } else {
                        self.before(&p, sub.decor().prefix().and_then(|r| r.as_str()));
                        self.after(&p, sub.decor().suffix().and_then(|r| r.as_str()));
                    }
                    self.table(sub, &p);
                }
                toml_edit::Item::ArrayOfTables(a) => {
                    for (i, sub) in a.iter().enumerate() {
                        let mut q = p.clone();
                        q.push(Seg::Index(i));
                        self.before(&q, sub.decor().prefix().and_then(|r| r.as_str()));
                        self.after(&q, sub.decor().suffix().and_then(|r| r.as_str()));
                        self.table(sub, &q);
                    }
                }
                toml_edit::Item::None => {}
            }
        }
    }

    fn value(&mut self, v: &toml_edit::Value, path: &Path) {
        self.after(path, v.decor().suffix().and_then(|r| r.as_str()));
        match v {
            toml_edit::Value::Array(a) => {
                let mut last: Option<Path> = None;
                for (i, item) in a.iter().enumerate() {
                    let mut p = path.clone();
                    p.push(Seg::Index(i));
                    let prefix = item.decor().prefix().and_then(|r| r.as_str()).unwrap_or("");
                    let (first, rest) = prefix.split_once('\n').unwrap_or((prefix, ""));
                    // What follows the comma, up to the line end, is about the item before.
                    match &last {
                        Some(prev) => self.after(prev, Some(first)),
                        None => self.after(path, Some(first)),
                    }
                    self.before(&p, Some(rest));
                    self.value(item, &p);
                    last = Some(p);
                }
                let trailing = a.trailing().as_str().unwrap_or("");
                let (first, rest) = trailing.split_once('\n').unwrap_or((trailing, ""));
                if let Some(prev) = &last {
                    self.after(prev, Some(first));
                }
                let (lines, _) = decor_lines(Some(rest));
                self.end(path, lines);
            }
            toml_edit::Value::InlineTable(t) => {
                for (k, item) in t.iter() {
                    let mut p = path.clone();
                    p.push(Seg::Key(k.to_string()));
                    self.value(item, &p);
                }
            }
            _ => {}
        }
    }
}

/// The comments of a TOML document.
pub fn toml_comments(text: &str) -> Option<Comments> {
    let doc: toml_edit::DocumentMut = text.parse().ok()?;
    let mut w = TomlWalk {
        c: Comments::default(),
        order: 0,
    };
    w.table(doc.as_table(), &Vec::new());
    let (lines, _) = decor_lines(doc.trailing().as_str());
    w.end(&[], lines);
    Some(w.c)
}

#[cfg(test)]
#[path = "comments_tests.rs"]
mod tests;
