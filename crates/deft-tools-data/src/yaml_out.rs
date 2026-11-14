//! A YAML writer for the value model. It quotes every string that a YAML 1.1
//! reader (PyYAML, Ruby, older Kubernetes tools) or a YAML 1.2 reader would
//! take for something else, and writes comments back next to their keys.

use crate::comments::{Comments, Note, Path, Seg};
use serde_json::{Map, Value};
use std::collections::HashMap;

pub struct Writer<'a> {
    pub comments: Option<&'a Comments>,
    /// Floats JSON cannot hold (nan, inf), by path; the value there is null.
    pub floats: &'a HashMap<Path, f64>,
    out: String,
}

impl<'a> Writer<'a> {
    pub fn new(comments: Option<&'a Comments>, floats: &'a HashMap<Path, f64>) -> Self {
        Writer {
            comments,
            floats,
            out: String::new(),
        }
    }

    fn note(&self, path: &[Seg]) -> Option<&'a Note> {
        self.comments.and_then(|c| c.get(path))
    }

    fn comment_lines(&mut self, lines: &[String], pad: usize) {
        for l in lines {
            self.out.push_str(&" ".repeat(pad));
            self.out.push('#');
            self.out.push_str(&crate::comments::clean(l));
            self.out.push('\n');
        }
    }

    fn before(&mut self, path: &[Seg], pad: usize) {
        if let Some(n) = self.note(path) {
            if n.blank && !self.out.is_empty() {
                self.out.push('\n');
            }
            let lines = n.before.clone();
            self.comment_lines(&lines, pad);
        }
    }

    fn end(&mut self, path: &[Seg], pad: usize) {
        if let Some(n) = self.note(path) {
            let lines = n.end.clone();
            self.comment_lines(&lines, pad);
        }
    }

    fn after(&mut self, path: &[Seg]) {
        if let Some(t) = self.note(path).and_then(|n| n.after.clone()) {
            self.out.push_str(" #");
            self.out.push_str(&crate::comments::clean(&t));
        }
    }

    /// Write a document.
    pub fn document(mut self, v: &Value) -> String {
        let root: Path = Vec::new();
        self.before(&root, 0);
        match v {
            Value::Object(m) if !m.is_empty() => self.map(m, &root, 0, None),
            Value::Array(a) if !a.is_empty() => self.seq(a, &root, 0),
            other => {
                let s = self.scalar(other, &root, 0);
                self.out.push_str(&s);
                self.after(&root);
                self.out.push('\n');
            }
        }
        self.end(&root, 0);
        self.out
    }

    fn scalar(&self, v: &Value, path: &[Seg], pad: usize) -> String {
        match v {
            Value::Null => match self.floats.get(path) {
                Some(f) if f.is_nan() => ".nan".into(),
                Some(f) if *f > 0.0 => ".inf".into(),
                Some(_) => "-.inf".into(),
                None => "null".into(),
            },
            Value::Bool(b) => b.to_string(),
            Value::Number(n) => number(n),
            Value::String(s) => string(s, pad),
            Value::Object(_) => "{}".into(),
            Value::Array(_) => "[]".into(),
        }
    }

    fn is_block(v: &Value) -> bool {
        match v {
            Value::Object(m) => !m.is_empty(),
            Value::Array(a) => !a.is_empty(),
            _ => false,
        }
    }

    /// Write the entries of a mapping. With `first`, the first key goes on the
    /// current line after that prefix (a list item's `- `).
    fn map(&mut self, m: &Map<String, Value>, path: &Path, pad: usize, first: Option<&str>) {
        for (i, (k, v)) in m.iter().enumerate() {
            let mut p = path.clone();
            p.push(Seg::Key(k.clone()));
            match (i, first) {
                (0, Some(prefix)) => self.out.push_str(prefix),
                _ => {
                    self.before(&p, pad);
                    self.out.push_str(&" ".repeat(pad));
                }
            }
            self.out.push_str(&key(k));
            self.out.push(':');
            self.value(v, &p, pad);
        }
        self.end(path, pad);
    }

    /// Write what follows `key:` or `- `.
    fn value(&mut self, v: &Value, p: &Path, pad: usize) {
        match v {
            Value::Object(sub) if Self::is_block(v) => {
                self.after(p);
                self.out.push('\n');
                self.map(sub, p, pad + 2, None);
            }
            Value::Array(items) if Self::is_block(v) => {
                self.after(p);
                self.out.push('\n');
                self.seq(items, p, pad);
            }
            _ => {
                let s = self.scalar(v, p, pad + 2);
                self.out.push(' ');
                if let Some((head, body)) = s.split_once('\n') {
                    self.out.push_str(head);
                    self.after(p);
                    self.out.push('\n');
                    self.out.push_str(body);
                    self.out.push('\n');
                } else {
                    self.out.push_str(&s);
                    self.after(p);
                    self.out.push('\n');
                }
            }
        }
    }

    fn seq(&mut self, a: &[Value], path: &Path, pad: usize) {
        for (i, v) in a.iter().enumerate() {
            let mut p = path.clone();
            p.push(Seg::Index(i));
            self.before(&p, pad);
            match v {
                Value::Object(m) if !m.is_empty() => {
                    // Comments above the first key go above the `- ` line.
                    let (k0, _) = m.iter().next().expect("not empty");
                    let mut first = p.clone();
                    first.push(Seg::Key(k0.clone()));
                    self.before(&first, pad);
                    if let Some(t) = self.note(&p).and_then(|n| n.after.clone()) {
                        self.comment_lines(&[t], pad);
                    }
                    let prefix = format!("{}- ", " ".repeat(pad));
                    self.map(m, &p, pad + 2, Some(&prefix));
                }
                Value::Array(inner) if !inner.is_empty() => {
                    self.out.push_str(&" ".repeat(pad));
                    self.out.push('-');
                    self.after(&p);
                    self.out.push('\n');
                    self.seq(inner, &p, pad + 2);
                }
                _ => {
                    self.out.push_str(&" ".repeat(pad));
                    self.out.push('-');
                    self.value(v, &p, pad);
                }
            }
        }
        self.end(path, pad);
    }
}

fn number(n: &serde_json::Number) -> String {
    let s = n.to_string();
    if !n.is_f64() {
        return s;
    }
    // YAML 1.1 readers need a dot and a signed exponent to see a float.
    let (mant, exp) = match s.split_once(['e', 'E']) {
        Some((m, e)) => (m.to_string(), Some(e.to_string())),
        None => (s.clone(), None),
    };
    let mant = if mant.contains('.') {
        mant
    } else {
        format!("{mant}.0")
    };
    match exp {
        Some(e) if e.starts_with(['-', '+']) => format!("{mant}e{e}"),
        Some(e) => format!("{mant}e+{e}"),
        None => mant,
    }
}

/// Characters YAML allows in a document. Everything else is escaped.
fn printable(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{7E}' | '\u{85}' | '\u{A0}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..)
        && c != '\u{FEFF}'
}

/// A line break to a YAML reader, though not to Rust's `lines`.
fn odd_break(c: char) -> bool {
    matches!(c, '\u{85}' | '\u{2028}' | '\u{2029}')
}

/// Text a YAML 1.1 or 1.2 reader would resolve to a non-string: a bool, a
/// null, a number (including 1:30 base 60 and 0x1F), a date, `<<` or `=`.
fn resolves(s: &str) -> bool {
    const WORDS: &[&str] = &[
        "y", "n", "yes", "no", "on", "off", "true", "false", "null", "~", "<<", "=",
    ];
    let lower = s.to_ascii_lowercase();
    if WORDS.contains(&lower.as_str()) {
        return true;
    }
    let body = lower.trim_start_matches(['-', '+']);
    if matches!(body, ".inf" | ".nan") {
        return true;
    }
    // Anything starting like a number and made only of the characters of
    // numbers, times and dates is quoted. Quoting too much is harmless.
    let starts = body.starts_with(|c: char| c.is_ascii_digit())
        || (body.starts_with('.') && body[1..].starts_with(|c: char| c.is_ascii_digit()));
    starts
        && body
            .chars()
            .all(|c| c.is_ascii_hexdigit() || "._:+-xobtz ".contains(c))
}

fn plain_ok(s: &str) -> bool {
    let Some(first) = s.chars().next() else {
        return false;
    };
    let last = s.chars().last().unwrap_or(first);
    !first.is_whitespace()
        && !last.is_whitespace()
        && !"-?:,[]{}#&*!|>'\"%@`".contains(first)
        && last != ':'
        && !s.starts_with("...")
        && !s.contains(": ")
        && !s.contains(" #")
        && !s.contains('\t')
        && s.chars()
            .all(|c| printable(c) && !odd_break(c) && c != '\n' && c != '\r')
        && !resolves(s)
}

fn single_ok(s: &str) -> bool {
    s.chars()
        .all(|c| printable(c) && !odd_break(c) && c != '\n' && c != '\r' && c != '\t')
}

fn double(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '\0' => out.push_str("\\0"),
            '\u{85}' => out.push_str("\\N"),
            '\u{2028}' => out.push_str("\\L"),
            '\u{2029}' => out.push_str("\\P"),
            c if (c as u32) < 0x100 && !printable(c) => {
                out.push_str(&format!("\\x{:02X}", c as u32))
            }
            c if !printable(c) => out.push_str(&format!("\\u{:04X}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A literal block (`|`) keeps multi-line text readable; only text that a
/// block cannot hold exactly is written in double quotes.
fn literal(s: &str, pad: usize) -> Option<String> {
    let fits = s.contains('\n')
        && s.trim() != ""
        && s.chars()
            .all(|c| printable(c) && !odd_break(c) && c != '\r');
    if !fits {
        return None;
    }
    let body = s.trim_end_matches('\n');
    let trailing = s.len() - body.len();
    let chomp = match trailing {
        0 => "-",
        1 => "",
        _ => "+",
    };
    let first = body.lines().find(|l| !l.is_empty()).unwrap_or("");
    // A first line that starts with a space needs an indentation indicator;
    // double quotes are simpler and as exact.
    if first.starts_with(' ') || body.starts_with('\n') {
        return None;
    }
    let indent = " ".repeat(pad);
    let mut out = format!("|{chomp}\n");
    let lines: Vec<&str> = s.strip_suffix('\n').unwrap_or(s).split('\n').collect();
    for l in lines {
        if !l.is_empty() {
            out.push_str(&indent);
            out.push_str(l);
        }
        out.push('\n');
    }
    // The caller ends the line itself.
    out.pop();
    Some(out)
}

fn string(s: &str, pad: usize) -> String {
    if plain_ok(s) {
        s.to_string()
    } else if let Some(block) = literal(s, pad) {
        block
    } else if single_ok(s) {
        format!("'{}'", s.replace('\'', "''"))
    } else {
        double(s)
    }
}

fn key(k: &str) -> String {
    if plain_ok(k) {
        k.to_string()
    } else if single_ok(k) {
        format!("'{}'", k.replace('\'', "''"))
    } else {
        double(k)
    }
}

/// Write a value as YAML.
pub fn to_yaml(v: &Value, comments: Option<&Comments>, floats: &HashMap<Path, f64>) -> String {
    Writer::new(comments, floats).document(v)
}

#[cfg(test)]
#[path = "yaml_out_tests.rs"]
mod tests;
