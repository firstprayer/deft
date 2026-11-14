//! A TOML writer for the value model that keeps key order and writes
//! comments back next to their keys and tables.

use crate::comments::{Comments, Note, Path, Seg, clean};
use serde_json::{Map, Value};
use std::collections::HashMap;

struct Writer<'a> {
    comments: Option<&'a Comments>,
    floats: &'a HashMap<Path, f64>,
    out: String,
}

/// A list of tables, written as `[[name]]` sections.
fn table_list(v: &Value) -> bool {
    matches!(v, Value::Array(a) if !a.is_empty() && a.iter().all(Value::is_object))
}

/// Entries that are written as their own `[section]`.
fn is_section(v: &Value) -> bool {
    v.is_object() || table_list(v)
}

fn bare(k: &str) -> bool {
    !k.is_empty()
        && k.chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn key(k: &str) -> String {
    if bare(k) { k.to_string() } else { basic(k) }
}

fn dotted(path: &[Seg]) -> String {
    path.iter()
        .filter_map(|s| match s {
            Seg::Key(k) => Some(key(k)),
            Seg::Index(_) => None,
        })
        .collect::<Vec<_>>()
        .join(".")
}

fn basic(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// Multi-line text as a `"""` string, which reads like the original.
fn multiline(s: &str) -> String {
    let mut out = String::from("\"\"\"\n");
    let chars: Vec<char> = s.chars().collect();
    let mut run = 0;
    for (i, &c) in chars.iter().enumerate() {
        if c == '"' {
            // Three quotes in a row, or one before the closing `"""`, would end the string.
            if run == 2 || i + 1 == chars.len() {
                out.push_str("\\\"");
                run = 0;
            } else {
                out.push('"');
                run += 1;
            }
            continue;
        }
        run = 0;
        match c {
            '\\' => out.push_str("\\\\"),
            '\n' | '\t' => out.push(c),
            '\r' => out.push_str("\\r"),
            c if c.is_control() => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push_str("\"\"\"");
    out
}

impl<'a> Writer<'a> {
    fn note(&self, path: &[Seg]) -> Option<&'a Note> {
        self.comments.and_then(|c| c.get(path))
    }

    fn lines(&mut self, lines: &[String], pad: &str) {
        for l in lines {
            self.out.push_str(pad);
            self.out.push('#');
            self.out.push_str(&clean(l));
            self.out.push('\n');
        }
    }

    fn before(&mut self, path: &[Seg], pad: &str, blank: bool) {
        let note = self.note(path);
        if (blank || note.is_some_and(|n| n.blank)) && !self.out.is_empty() {
            self.out.push('\n');
        }
        if let Some(n) = note {
            self.lines(&n.before.clone(), pad);
        }
    }

    fn after(&mut self, path: &[Seg]) {
        if let Some(t) = self.note(path).and_then(|n| n.after.clone()) {
            self.out.push_str(" #");
            self.out.push_str(&clean(&t));
        }
    }

    fn end(&mut self, path: &[Seg], pad: &str) {
        if let Some(n) = self.note(path) {
            self.lines(&n.end.clone(), pad);
        }
    }

    /// Comments of nodes inside an inline value, which has no room for
    /// them: they go above the line that holds it.
    fn inner(&mut self, path: &[Seg], pad: &str) {
        let Some(c) = self.comments else { return };
        let mut lines = Vec::new();
        for (_, n) in c.below(path) {
            lines.extend(n.before.iter().cloned());
            lines.extend(n.after.iter().cloned());
            lines.extend(n.end.iter().cloned());
        }
        self.lines(&lines, pad);
    }

    fn inline(&self, v: &Value, path: &[Seg]) -> String {
        match v {
            Value::Null => match self.floats.get(path) {
                Some(f) if f.is_nan() => "nan".into(),
                Some(f) if *f > 0.0 => "inf".into(),
                Some(_) => "-inf".into(),
                None => "\"\"".into(),
            },
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => n.to_string(),
            Value::String(s) => basic(s),
            Value::Array(a) => {
                let items: Vec<String> = a
                    .iter()
                    .enumerate()
                    .map(|(i, x)| self.inline(x, &child(path, Seg::Index(i))))
                    .collect();
                format!("[{}]", items.join(", "))
            }
            Value::Object(m) => {
                if m.is_empty() {
                    return "{}".into();
                }
                let items: Vec<String> = m
                    .iter()
                    .map(|(k, x)| {
                        format!(
                            "{} = {}",
                            key(k),
                            self.inline(x, &child(path, Seg::Key(k.clone())))
                        )
                    })
                    .collect();
                format!("{{ {} }}", items.join(", "))
            }
        }
    }

    /// An array as one line, or one item per line when it is long or its
    /// items have comments.
    fn array(&mut self, a: &[Value], path: &Path) {
        let one = self.inline(&Value::Array(a.to_vec()), path);
        let commented = self.comments.is_some_and(|c| !c.below(path).is_empty());
        if !commented && one.len() <= 100 {
            self.out.push_str(&one);
            self.after(path);
            self.out.push('\n');
            return;
        }
        self.out.push('[');
        self.after(path);
        self.out.push('\n');
        for (i, x) in a.iter().enumerate() {
            let p = child(path, Seg::Index(i));
            self.before(&p, "    ", false);
            self.inner(&p, "    ");
            self.out.push_str("    ");
            self.out.push_str(&self.inline(x, &p));
            self.out.push(',');
            self.after(&p);
            self.out.push('\n');
        }
        self.end(path, "    ");
        self.out.push_str("]\n");
    }

    fn entry(&mut self, k: &str, v: &Value, path: &Path) {
        self.before(path, "", false);
        self.out.push_str(&key(k));
        self.out.push_str(" = ");
        match v {
            Value::Array(a) if !a.is_empty() => self.array(a, path),
            Value::String(s) if s.contains('\n') => {
                self.out.push_str(&multiline(s));
                self.after(path);
                self.out.push('\n');
            }
            _ => {
                let text = self.inline(v, path);
                self.out.push_str(&text);
                self.after(path);
                self.out.push('\n');
            }
        }
    }

    /// The body of a table: plain keys first, then its sections.
    fn table(&mut self, m: &Map<String, Value>, path: &Path) {
        for (k, v) in m.iter().filter(|(_, v)| !is_section(v)) {
            let p = child(path, Seg::Key(k.clone()));
            self.entry(k, v, &p);
        }
        if !path.is_empty() {
            self.end(path, "");
        }
        for (k, v) in m.iter().filter(|(_, v)| is_section(v)) {
            let p = child(path, Seg::Key(k.clone()));
            match v {
                Value::Object(sub) => {
                    let has_plain = sub.values().any(|x| !is_section(x));
                    let commented = self.note(&p).is_some();
                    if has_plain || sub.is_empty() || commented {
                        self.before(&p, "", true);
                        self.out.push_str(&format!("[{}]", dotted(&p)));
                        self.after(&p);
                        self.out.push('\n');
                    }
                    self.table(sub, &p);
                }
                Value::Array(items) => {
                    self.before(&p, "", true);
                    for (i, item) in items.iter().enumerate() {
                        let q = child(&p, Seg::Index(i));
                        self.before(&q, "", i > 0);
                        self.out.push_str(&format!("[[{}]]", dotted(&p)));
                        self.after(&q);
                        self.out.push('\n');
                        if let Value::Object(sub) = item {
                            self.table(sub, &q);
                        }
                    }
                    self.end(&p, "");
                }
                _ => unreachable!("sections are tables or lists of tables"),
            }
        }
    }
}

fn child(path: &[Seg], s: Seg) -> Path {
    let mut p = path.to_vec();
    p.push(s);
    p
}

/// Write a table as TOML. Nulls must already be gone.
pub fn to_toml(
    m: &Map<String, Value>,
    comments: Option<&Comments>,
    floats: &HashMap<Path, f64>,
) -> String {
    let mut w = Writer {
        comments,
        floats,
        out: String::new(),
    };
    let root: Path = Vec::new();
    if let Some(n) = w.note(&root) {
        w.lines(&n.before.clone(), "");
    }
    w.table(m, &root);
    w.end(&root, "");
    w.out
}

#[cfg(test)]
#[path = "toml_out_tests.rs"]
mod tests;
