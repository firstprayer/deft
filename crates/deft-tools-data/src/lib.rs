//! Structured data capabilities: read and write JSON, JSON Lines, YAML, TOML
//! and XML through one value model, query with jq syntax, merge and diff.
//! Comments of YAML and TOML ride beside the value model, so a conversion
//! between the two keeps them.

use deft_core::error::{ErrorCode, Result, err};
use deft_core::format::Format;
use serde_json::{Value, json};
use std::collections::HashMap;

pub mod comments;
mod diff;
pub mod geo;
mod query;
mod schema;
mod toml_format;
mod toml_out;
mod xml;
mod yaml_out;

pub use comments::{Comments, Path, Seg};
pub use diff::{Change, diff};
pub use query::query;
pub use schema::Checked;

/// Tidy TOML text, keeping its comments; see `toml_format`.
pub fn format_toml(text: &str, sort: bool) -> Result<String> {
    toml_format::format(text, sort).map_err(|e| bad(Format::Toml, e))
}

fn bad(format: Format, e: impl std::fmt::Display) -> deft_core::error::DeftError {
    err(ErrorCode::InputCorrupt, format!("invalid {format}: {e}"))
}

/// A document as read: its value, its comments and what the value model
/// could not hold as it was.
#[derive(Clone, Debug, Default)]
pub struct Doc {
    pub value: Value,
    pub comments: Comments,
    /// The source has comments that could not be tied to a node.
    pub comments_unplaced: bool,
    /// nan and inf, which JSON cannot hold: null in `value`, the float here.
    pub floats: HashMap<Path, f64>,
    /// What reading changed, worded as warnings.
    pub warnings: Vec<String>,
}

impl Doc {
    pub fn plain(value: Value) -> Doc {
        Doc {
            value,
            ..Doc::default()
        }
    }

    fn has_comments(&self) -> bool {
        self.comments_unplaced || !self.comments.is_empty()
    }
}

/// Parse a document. Multi-document YAML becomes an array.
pub fn parse(bytes: &[u8], format: Format) -> Result<Value> {
    read(bytes, format).map(|d| d.value)
}

/// Read a document, with its comments.
pub fn read(bytes: &[u8], format: Format) -> Result<Doc> {
    read_with(bytes, format, false)
}

/// Read a document; JSON may have comments and trailing commas (JSONC, as
/// in VS Code settings and tsconfig.json), which are dropped with a warning.
pub fn read_lenient(bytes: &[u8], format: Format) -> Result<Doc> {
    read_with(bytes, format, true)
}

fn read_with(bytes: &[u8], format: Format, jsonc: bool) -> Result<Doc> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let text = || std::str::from_utf8(bytes).map_err(|e| bad(format, e));
    match format {
        Format::Json => match serde_json::from_slice(bytes) {
            Ok(v) => Ok(Doc::plain(v)),
            Err(e) => {
                let Some(clean) = std::str::from_utf8(bytes).ok().and_then(strip_jsonc) else {
                    return Err(bad(format, e));
                };
                let Ok(v) = serde_json::from_str(&clean) else {
                    return Err(bad(format, e));
                };
                if !jsonc {
                    return Err(bad(
                        format,
                        format!(
                            "comments or trailing commas are not allowed in JSON (line {} column {}); this is JSON with comments (JSONC)",
                            e.line(),
                            e.column()
                        ),
                    ));
                }
                let mut doc = Doc::plain(v);
                doc.warnings
                    .push("read as JSON with comments (JSONC): comments are not kept".into());
                Ok(doc)
            }
        },
        Format::Jsonl => text()?
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty())
            .map(|(n, l)| {
                serde_json::from_str(l).map_err(|e| bad(format, format!("line {}: {e}", n + 1)))
            })
            .collect::<Result<Vec<Value>>>()
            .map(|v| Doc::plain(Value::Array(v))),
        Format::Yaml => read_yaml(text()?),
        Format::Toml => {
            let text = text()?;
            let v: toml::Value = toml::from_str(text).map_err(|e| bad(format, e))?;
            let mut floats = HashMap::new();
            let value = from_toml(v, &mut Vec::new(), &mut floats);
            let found = comments::toml_comments(text);
            Ok(Doc {
                value,
                floats,
                comments_unplaced: found.is_none() && text.lines().any(has_comment),
                comments: found.unwrap_or_default(),
                ..Doc::default()
            })
        }
        Format::Xml => {
            let text = text()?;
            let mut doc = Doc::plain(xml::parse(text).map_err(|e| bad(format, e))?);
            doc.warnings = xml::lost(text);
            Ok(doc)
        }
        other => Err(err(
            ErrorCode::InputUnsupported,
            format!("cannot read {other} as data"),
        )),
    }
}

/// JSON with `//` and `/* */` comments and trailing commas made plain.
/// `None` when there was nothing to remove.
fn strip_jsonc(text: &str) -> Option<String> {
    let b = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let (mut i, mut changed) = (0, false);
    let mut last = 0;
    while i < b.len() {
        match b[i] {
            b'"' => {
                let mut j = i + 1;
                while j < b.len() && b[j] != b'"' {
                    j += if b[j] == b'\\' { 2 } else { 1 };
                }
                i = (j + 1).min(b.len());
            }
            b'/' if b.get(i + 1) == Some(&b'/') => {
                out.push_str(&text[last..i]);
                let end = text[i..].find('\n').map_or(b.len(), |n| i + n);
                (i, last, changed) = (end, end, true);
            }
            b'/' if b.get(i + 1) == Some(&b'*') => {
                out.push_str(&text[last..i]);
                let end = text[i + 2..].find("*/").map_or(b.len(), |n| i + 2 + n + 2);
                out.push(' ');
                (i, last, changed) = (end, end, true);
            }
            b',' => {
                // A trailing comma: the next thing past spaces and comments closes.
                if matches!(next_token(&text[i + 1..]), Some(b'}' | b']')) {
                    out.push_str(&text[last..i]);
                    (last, changed) = (i + 1, true);
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    out.push_str(&text[last.min(text.len())..]);
    changed.then_some(out)
}

/// The first byte of `text` that is not a space or inside a comment.
fn next_token(mut text: &str) -> Option<u8> {
    loop {
        text = text.trim_start();
        if let Some(rest) = text.strip_prefix("//") {
            text = rest.find('\n').map_or("", |n| &rest[n..]);
        } else if let Some(rest) = text.strip_prefix("/*") {
            text = rest.find("*/").map_or("", |n| &rest[n + 2..]);
        } else {
            return text.bytes().next();
        }
    }
}

/// Documents, nan and inf by path, tags, and a note when duplicate keys
/// were resolved.
type YamlRead = (Vec<Value>, HashMap<Path, f64>, Vec<String>, Option<String>);

fn yaml_values(text: &str) -> Result<YamlRead> {
    use serde::Deserialize;
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(text);
    let mut raw = Vec::new();
    let mut duplicate = None;
    for doc in serde_yaml::Deserializer::from_str(text) {
        match serde_yaml::Value::deserialize(doc) {
            Ok(v) => raw.push(v),
            // YAML forbids a key twice in one mapping, but real files have
            // it and PyYAML, Ruby and Go readers take the last value.
            Err(e) if e.to_string().contains("duplicate entry") => {
                raw = loose_yaml(text).ok_or_else(|| bad(Format::Yaml, &e))?;
                duplicate = Some(format!(
                    "a key appears twice in one mapping ({}); the last value is used, as most YAML readers do",
                    e.to_string().split(" at line").next().unwrap_or_default()
                ));
                break;
            }
            Err(e) => return Err(bad(Format::Yaml, e)),
        }
    }
    let mut docs = Vec::new();
    let mut floats = HashMap::new();
    let mut tags = Vec::new();
    for (i, mut v) in raw.into_iter().enumerate() {
        merge_keys(&mut v).map_err(|e| bad(Format::Yaml, e))?;
        let mut path = vec![Seg::Index(i)];
        docs.push(from_yaml(v, &mut path, &mut floats, &mut tags));
    }
    Ok((docs, floats, tags, duplicate))
}

/// Read YAML that serde_yaml refuses for a repeated key, keeping the last.
fn loose_yaml(text: &str) -> Option<Vec<serde_yaml::Value>> {
    use yaml_rust::Yaml;
    fn conv(y: Yaml) -> serde_yaml::Value {
        use serde_yaml::Value as Y;
        match y {
            Yaml::Real(s) => match s
                .trim_start_matches(['+', '-'])
                .to_ascii_lowercase()
                .as_str()
            {
                ".inf" => Y::Number(
                    if s.starts_with('-') {
                        f64::NEG_INFINITY
                    } else {
                        f64::INFINITY
                    }
                    .into(),
                ),
                ".nan" => Y::Number(f64::NAN.into()),
                _ => s
                    .parse::<f64>()
                    .map_or(Y::String(s), |f| Y::Number(f.into())),
            },
            Yaml::Integer(i) => Y::Number(i.into()),
            Yaml::String(s) => Y::String(s),
            Yaml::Boolean(b) => Y::Bool(b),
            Yaml::Array(a) => Y::Sequence(a.into_iter().map(conv).collect()),
            Yaml::Hash(h) => Y::Mapping(h.into_iter().map(|(k, v)| (conv(k), conv(v))).collect()),
            Yaml::Alias(_) | Yaml::Null | Yaml::BadValue => Y::Null,
        }
    }
    let docs = yaml_rust::YamlLoader::load_from_str(text).ok()?;
    Some(docs.into_iter().map(conv).collect())
}

fn read_yaml(text: &str) -> Result<Doc> {
    let (mut docs, floats, tags, duplicate) = yaml_values(text)?;
    let n = docs.len();
    // One document is the value itself; several are a list.
    let floats = floats
        .into_iter()
        .map(|(mut p, f)| {
            if n == 1 {
                p.remove(0);
            }
            (p, f)
        })
        .collect();
    let value = match n {
        0 => Value::Null,
        1 => docs.remove(0),
        _ => Value::Array(docs),
    };
    let mut doc = Doc {
        value,
        floats,
        ..Doc::default()
    };
    let (found, typed) = comments::yaml_extras(text, n);
    match found {
        Some(c) => doc.comments = c,
        None => doc.comments_unplaced = text.lines().any(has_comment),
    }
    doc.warnings.extend(duplicate);
    let mut tags = tags;
    for (path, tag) in typed {
        // `!!python/name:module.function` says everything in its tag.
        if let Some(slot) = comments::lookup_mut(&mut doc.value, &path)
            && (slot.is_null() || slot.as_str() == Some(""))
        {
            *slot = Value::String(tag.clone());
        }
        tags.push(tag);
    }
    for tag in comments::yaml_text_tags(text) {
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    if !tags.is_empty() {
        let mut shown: Vec<&str> = Vec::new();
        for t in &tags {
            if !shown.contains(&t.as_str()) && shown.len() < 3 {
                shown.push(t);
            }
        }
        doc.warnings.push(format!(
            "YAML tags are not kept on {} value{} ({}); a value that was only a tag is written as the tag's text",
            tags.len(),
            if tags.len() == 1 { "" } else { "s" },
            shown.join(", ")
        ));
    }
    Ok(doc)
}

/// Apply YAML merge keys (`<<: *base` and `<<: [*a, *b]`) everywhere,
/// innermost first, so a merged mapping's own merge keys count too. Keys
/// written in the mapping win, then earlier sources over later ones.
fn merge_keys(v: &mut serde_yaml::Value) -> std::result::Result<(), String> {
    use serde_yaml::Value as Y;
    match v {
        Y::Mapping(m) => {
            for (_, x) in m.iter_mut() {
                merge_keys(x)?;
            }
            let merge = Y::String("<<".into());
            if !m.contains_key(&merge) {
                return Ok(());
            }
            let old = std::mem::take(m);
            let explicit: Vec<Y> = old.keys().filter(|k| **k != merge).cloned().collect();
            for (k, x) in old {
                if k != merge {
                    m.insert(k, x);
                    continue;
                }
                let sources = match x {
                    Y::Mapping(s) => vec![s],
                    Y::Sequence(list) => list
                        .into_iter()
                        .map(|s| match s {
                            Y::Mapping(s) => Ok(s),
                            _ => Err("a merge key (<<) list may only hold mappings".to_string()),
                        })
                        .collect::<std::result::Result<_, _>>()?,
                    Y::Null => vec![],
                    _ => return Err("a merge key (<<) must name a mapping".into()),
                };
                for s in sources {
                    for (sk, sv) in s {
                        if !explicit.contains(&sk) && !m.contains_key(&sk) {
                            m.insert(sk, sv);
                        }
                    }
                }
            }
            Ok(())
        }
        Y::Sequence(s) => s.iter_mut().try_for_each(merge_keys),
        Y::Tagged(t) => merge_keys(&mut t.value),
        _ => Ok(()),
    }
}

fn yaml_key(k: serde_yaml::Value) -> String {
    match from_yaml(k, &mut Vec::new(), &mut HashMap::new(), &mut Vec::new()) {
        Value::String(s) => s,
        other => other.to_string(),
    }
}

/// A tag as it is usually written: `!!python/name:x` rather than its full
/// `tag:yaml.org,2002:` form.
fn tag_text(tag: &serde_yaml::value::Tag) -> String {
    let t = tag.to_string();
    let t = t.strip_prefix('!').unwrap_or(&t);
    match t.strip_prefix("tag:yaml.org,2002:") {
        Some(rest) => format!("!!{rest}"),
        None => format!("!{t}"),
    }
}

fn from_yaml(
    v: serde_yaml::Value,
    path: &mut Path,
    floats: &mut HashMap<Path, f64>,
    tags: &mut Vec<String>,
) -> Value {
    use serde_yaml::Value as Y;
    match v {
        Y::Null => Value::Null,
        Y::Bool(b) => Value::Bool(b),
        Y::Number(n) => {
            if let Some(f) = n.as_f64().filter(|f| !f.is_finite()) {
                floats.insert(path.clone(), f);
                return Value::Null;
            }
            serde_json::to_value(&n).unwrap_or(Value::Null)
        }
        Y::String(s) => Value::String(s),
        Y::Sequence(s) => Value::Array(
            s.into_iter()
                .enumerate()
                .map(|(i, x)| {
                    path.push(Seg::Index(i));
                    let v = from_yaml(x, path, floats, tags);
                    path.pop();
                    v
                })
                .collect(),
        ),
        Y::Mapping(m) => Value::Object(
            m.into_iter()
                .map(|(k, x)| {
                    let k = yaml_key(k);
                    path.push(Seg::Key(k.clone()));
                    let v = from_yaml(x, path, floats, tags);
                    path.pop();
                    (k, v)
                })
                .collect(),
        ),
        Y::Tagged(t) => {
            let text = tag_text(&t.tag);
            tags.push(text.clone());
            match t.value {
                // `!!python/name:module.function` says everything in its tag.
                Y::Null => Value::String(text),
                Y::String(s) if s.is_empty() => Value::String(text),
                other => from_yaml(other, path, floats, tags),
            }
        }
    }
}

fn from_toml(v: toml::Value, path: &mut Path, floats: &mut HashMap<Path, f64>) -> Value {
    use toml::Value as T;
    match v {
        T::String(s) => Value::String(s),
        T::Integer(i) => json!(i),
        T::Float(f) if !f.is_finite() => {
            floats.insert(path.clone(), f);
            Value::Null
        }
        T::Float(f) => json!(f),
        T::Boolean(b) => Value::Bool(b),
        T::Datetime(d) => Value::String(d.to_string()),
        T::Array(a) => Value::Array(
            a.into_iter()
                .enumerate()
                .map(|(i, x)| {
                    path.push(Seg::Index(i));
                    let v = from_toml(x, path, floats);
                    path.pop();
                    v
                })
                .collect(),
        ),
        T::Table(t) => Value::Object(
            t.into_iter()
                .map(|(k, x)| {
                    path.push(Seg::Key(k.clone()));
                    let v = from_toml(x, path, floats);
                    path.pop();
                    (k, v)
                })
                .collect(),
        ),
    }
}

/// True when a line of YAML or TOML has a comment, outside quotes.
fn has_comment(line: &str) -> bool {
    let mut quote = None;
    let mut before = ' ';
    for c in line.chars() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '#' && before.is_whitespace() => return true,
            None => {}
        }
        before = c;
    }
    false
}

/// A YAML value such as 1.10, which reads as the number 1.1.
fn has_trailing_zero(line: &str) -> bool {
    let value = line.rsplit([':', '-']).next().unwrap_or_default().trim();
    let number = value.split_once('.').is_some_and(|(a, b)| {
        !a.is_empty() && a.bytes().chain(b.bytes()).all(|c| c.is_ascii_digit())
    });
    number && value.ends_with('0')
}

/// A YAML anchor or alias, such as `base: &b` or `- *b`.
fn has_anchor(line: &str) -> bool {
    let marked = |s: &str| s.trim_start().starts_with(['&', '*']);
    line.split_once(": ").is_some_and(|(_, v)| marked(v))
        || line.trim_start().strip_prefix("- ").is_some_and(marked)
}

/// A whole number too long for 64 bits, outside any string. It is read as a
/// floating-point number, which cannot hold every digit.
fn has_huge_integer(text: &str) -> bool {
    let (mut quoted, mut escaped, mut run) = (false, false, 0);
    for c in text.chars() {
        match c {
            _ if escaped => escaped = false,
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            c if !quoted && c.is_ascii_digit() => {
                run += 1;
                if run > 20 {
                    return true;
                }
                continue;
            }
            _ => {}
        }
        run = 0;
    }
    false
}

/// What reading a document changes, besides comments, because the value
/// model cannot hold it.
pub fn lost_on_reading(bytes: &[u8], format: Format) -> Vec<String> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return vec![];
    };
    let mut lost = Vec::new();
    if format == Format::Yaml && text.lines().any(has_trailing_zero) {
        lost.push("a number such as 1.10 is written 1.1; quote it to keep the zero".to_string());
    }
    if format == Format::Yaml && text.lines().any(has_anchor) {
        lost.push("anchors are written out in full".to_string());
    }
    if matches!(format, Format::Json | Format::Jsonl) && has_huge_integer(text) {
        lost.push("a whole number over 20 digits loses its last digits".to_string());
    }
    lost
}

/// How `render` lays out a document.
#[derive(Clone, Copy, Debug, Default)]
pub struct Style {
    /// JSON on one line.
    pub compact: bool,
    pub sort_keys: bool,
    /// Spaces per level for JSON; 0 means the default of 2.
    pub indent: usize,
}

pub fn sort_keys(v: &mut Value) {
    match v {
        Value::Object(m) => {
            let mut pairs: Vec<(String, Value)> = std::mem::take(m).into_iter().collect();
            pairs.sort_by(|a, b| a.0.cmp(&b.0));
            for (k, mut val) in pairs {
                sort_keys(&mut val);
                m.insert(k, val);
            }
        }
        Value::Array(a) => a.iter_mut().for_each(sort_keys),
        _ => {}
    }
}

/// TOML has no null: drop those keys and list items (but not nan and inf,
/// which are null in the value model), and report how many went.
fn drop_nulls(v: &mut Value, path: &mut Path, keep: &HashMap<Path, f64>) -> usize {
    match v {
        Value::Object(m) => {
            let mut n = 0;
            let old = std::mem::take(m);
            for (k, mut x) in old {
                path.push(Seg::Key(k.clone()));
                if x.is_null() && !keep.contains_key(path) {
                    n += 1;
                } else {
                    n += drop_nulls(&mut x, path, keep);
                    m.insert(k, x);
                }
                path.pop();
            }
            n
        }
        Value::Array(a) => {
            let mut n = 0;
            for (i, x) in a.iter_mut().enumerate() {
                path.push(Seg::Index(i));
                n += drop_nulls(x, path, keep);
                path.pop();
            }
            let before = a.len();
            let mut i = 0;
            a.retain(|x| {
                path.push(Seg::Index(i));
                let keep_it = !x.is_null() || keep.contains_key(path);
                path.pop();
                i += 1;
                keep_it
            });
            n + before - a.len()
        }
        _ => 0,
    }
}

fn json_text(v: &Value, style: Style) -> Result<String> {
    use serde::Serialize;
    if style.compact {
        return serde_json::to_string(v).map_err(|e| err(ErrorCode::Internal, e.to_string()));
    }
    let pad = " ".repeat(if style.indent == 0 { 2 } else { style.indent });
    let mut out = Vec::new();
    let fmt = serde_json::ser::PrettyFormatter::with_indent(pad.as_bytes());
    let mut ser = serde_json::Serializer::with_formatter(&mut out, fmt);
    v.serialize(&mut ser)
        .map_err(|e| err(ErrorCode::Internal, e.to_string()))?;
    String::from_utf8(out).map_err(|e| err(ErrorCode::Internal, e.to_string()))
}

/// Write a document. Returns its bytes and any data the format could not hold.
pub fn render(value: &Value, format: Format, style: Style) -> Result<(Vec<u8>, Vec<String>)> {
    render_doc(&Doc::plain(value.clone()), format, style)
}

/// Write a document with its comments where the format can hold them.
pub fn render_doc(doc: &Doc, format: Format, style: Style) -> Result<(Vec<u8>, Vec<String>)> {
    let mut v = doc.value.clone();
    let mut comments = doc.comments.clone();
    if style.sort_keys {
        sort_keys(&mut v);
    }
    let mut warnings = Vec::new();
    let unable = |e: &dyn std::fmt::Display| {
        err(
            ErrorCode::InputUnsupported,
            format!("cannot write as {format}: {e}"),
        )
    };
    let keeps_comments = matches!(format, Format::Yaml | Format::Toml);
    if doc.has_comments() && (!keeps_comments || doc.comments_unplaced) {
        warnings.push("comments are not kept".to_string());
    }
    if !doc.floats.is_empty() && !matches!(format, Format::Yaml | Format::Toml) {
        warnings.push(format!(
            "{} has no nan or inf: {} value{} written as null",
            format.label().to_uppercase(),
            doc.floats.len(),
            if doc.floats.len() == 1 { "" } else { "s" }
        ));
    }
    let text = match format {
        Format::Json => json_text(&v, style)? + "\n",
        Format::Jsonl => {
            let rows = match &v {
                Value::Array(a) => a.clone(),
                other => vec![other.clone()],
            };
            let mut s = String::new();
            for row in &rows {
                s.push_str(&serde_json::to_string(row).map_err(|e| unable(&e))?);
                s.push('\n');
            }
            s
        }
        Format::Yaml => {
            comments.rehome(&v);
            yaml_out::to_yaml(&v, Some(&comments), &doc.floats)
        }
        Format::Toml => {
            let Value::Object(_) = v else {
                return Err(err(
                    ErrorCode::InputUnsupported,
                    "TOML needs a table at the top; this document is a list or a single value",
                )
                .hint("wrap the value in an object first"));
            };
            let dropped = drop_nulls(&mut v, &mut Vec::new(), &doc.floats);
            if dropped > 0 {
                warnings.push(format!("TOML has no null: dropped {dropped} keys"));
            }
            comments.rehome(&v);
            let Value::Object(m) = &v else { unreachable!() };
            toml_out::to_toml(m, Some(&comments), &doc.floats)
        }
        Format::Xml => xml::render(&v),
        other => {
            return Err(err(
                ErrorCode::InputUnsupported,
                format!("cannot write data as {other}"),
            ));
        }
    };
    Ok((text.into_bytes(), warnings))
}

/// Merge `b` into `a`: objects merge key by key, anything else is replaced.
/// With `append`, arrays are joined instead of replaced.
pub fn merge(a: &mut Value, b: Value, append: bool) {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (k, v) in y {
                match x.get_mut(&k) {
                    Some(old) => merge(old, v, append),
                    None => {
                        x.insert(k, v);
                    }
                }
            }
        }
        (Value::Array(x), Value::Array(y)) if append => x.extend(y),
        (slot, other) => *slot = other,
    }
}

/// Check a document against a JSON Schema in the file at `location`.
pub fn validate_schema(
    value: &Value,
    schema: &Value,
    location: Option<&std::path::Path>,
) -> Result<Checked> {
    schema::validate(value, schema, location)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
