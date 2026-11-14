//! Structured data capabilities: read and write JSON, JSON Lines, YAML, TOML
//! and XML through one value model, query with jq syntax, merge and diff.

use deft_core::error::{ErrorCode, Result, err};
use deft_core::format::Format;
use serde_json::{Map, Value, json};

mod diff;
pub mod geo;
mod query;
mod xml;

pub use diff::{Change, diff};
pub use query::query;

fn bad(format: Format, e: impl std::fmt::Display) -> deft_core::error::DeftError {
    err(ErrorCode::InputCorrupt, format!("invalid {format}: {e}"))
}

/// Parse a document. Multi-document YAML becomes an array.
pub fn parse(bytes: &[u8], format: Format) -> Result<Value> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    let text = || std::str::from_utf8(bytes).map_err(|e| bad(format, e));
    match format {
        Format::Json => serde_json::from_slice(bytes).map_err(|e| bad(format, e)),
        Format::Jsonl => text()?
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty())
            .map(|(n, l)| {
                serde_json::from_str(l).map_err(|e| bad(format, format!("line {}: {e}", n + 1)))
            })
            .collect::<Result<Vec<Value>>>()
            .map(Value::Array),
        Format::Yaml => parse_yaml(bytes),
        Format::Toml => {
            let v: toml::Value = toml::from_str(text()?).map_err(|e| bad(format, e))?;
            Ok(from_toml(v))
        }
        Format::Xml => xml::parse(text()?).map_err(|e| bad(format, e)),
        other => Err(err(
            ErrorCode::InputUnsupported,
            format!("cannot read {other} as data"),
        )),
    }
}

/// Every document of a YAML file.
pub fn yaml_documents(bytes: &[u8]) -> Result<Vec<Value>> {
    use serde::Deserialize;
    let mut docs = Vec::new();
    for doc in serde_yaml::Deserializer::from_slice(bytes) {
        let mut v = serde_yaml::Value::deserialize(doc).map_err(|e| bad(Format::Yaml, e))?;
        v.apply_merge().map_err(|e| bad(Format::Yaml, e))?;
        docs.push(from_yaml(v));
    }
    Ok(docs)
}

fn parse_yaml(bytes: &[u8]) -> Result<Value> {
    let mut docs = yaml_documents(bytes)?;
    Ok(match docs.len() {
        0 => Value::Null,
        1 => docs.remove(0),
        _ => Value::Array(docs),
    })
}

fn yaml_key(k: serde_yaml::Value) -> String {
    match from_yaml(k) {
        Value::String(s) => s,
        other => other.to_string(),
    }
}

fn from_yaml(v: serde_yaml::Value) -> Value {
    use serde_yaml::Value as Y;
    match v {
        Y::Null => Value::Null,
        Y::Bool(b) => Value::Bool(b),
        Y::Number(n) => serde_json::to_value(&n).unwrap_or(Value::Null),
        Y::String(s) => Value::String(s),
        Y::Sequence(s) => Value::Array(s.into_iter().map(from_yaml).collect()),
        Y::Mapping(m) => Value::Object(
            m.into_iter()
                .map(|(k, v)| (yaml_key(k), from_yaml(v)))
                .collect(),
        ),
        Y::Tagged(t) => from_yaml(t.value),
    }
}

fn from_toml(v: toml::Value) -> Value {
    use toml::Value as T;
    match v {
        T::String(s) => Value::String(s),
        T::Integer(i) => json!(i),
        T::Float(f) => json!(f),
        T::Boolean(b) => Value::Bool(b),
        T::Datetime(d) => Value::String(d.to_string()),
        T::Array(a) => Value::Array(a.into_iter().map(from_toml).collect()),
        T::Table(t) => Value::Object(t.into_iter().map(|(k, v)| (k, from_toml(v))).collect()),
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

/// What reading a document drops, because the value model cannot hold it.
pub fn lost_on_reading(bytes: &[u8], format: Format) -> Vec<String> {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return vec![];
    };
    let mut lost = Vec::new();
    if matches!(format, Format::Yaml | Format::Toml) && text.lines().any(has_comment) {
        lost.push("comments are not kept".to_string());
    }
    if format == Format::Yaml && text.lines().any(has_trailing_zero) {
        lost.push("a number such as 1.10 is written 1.1; quote it to keep the zero".to_string());
    }
    if format == Format::Yaml && text.lines().any(has_anchor) {
        lost.push("anchors are written out in full".to_string());
    }
    if matches!(format, Format::Json | Format::Jsonl) && has_huge_integer(text) {
        lost.push("a whole number over 20 digits loses its last digits".to_string());
    }
    if format == Format::Xml {
        lost.extend(xml::lost(text));
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

/// TOML has no null: drop those keys, and report how many went.
fn drop_nulls(v: &mut Value) -> usize {
    match v {
        Value::Object(m) => {
            let before = m.len();
            m.retain(|_, x| !x.is_null());
            before - m.len() + m.values_mut().map(drop_nulls).sum::<usize>()
        }
        Value::Array(a) => a.iter_mut().map(drop_nulls).sum(),
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
    let mut v = value.clone();
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
        Format::Yaml => to_yaml(&v).map_err(|e| unable(&e))?,
        Format::Toml => {
            if !v.is_object() {
                return Err(err(
                    ErrorCode::InputUnsupported,
                    "TOML needs a table at the top; this document is a list or a single value",
                )
                .hint("wrap the value in an object first"));
            }
            let dropped = drop_nulls(&mut v);
            if dropped > 0 {
                warnings.push(format!("TOML has no null: dropped {dropped} keys"));
            }
            toml::to_string_pretty(&v).map_err(|e| unable(&e))?
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

/// YAML 1.1 parsers such as PyYAML read `yes`, `on` and `2026-11-06` as a bool or a
/// date. serde_yaml writes them bare, because they are plain strings in YAML 1.2.
fn ambiguous_in_yaml(s: &str) -> bool {
    const WORDS: &[&str] = &["y", "n", "yes", "no", "on", "off"];
    let digits = |t: &str| !t.is_empty() && t.bytes().all(|b| b.is_ascii_digit());
    let date = s.len() >= 10
        && s.is_char_boundary(10)
        && digits(&s[..4])
        && digits(&s[5..7])
        && digits(&s[8..10])
        && s.as_bytes()[4] == b'-'
        && s.as_bytes()[7] == b'-';
    let sexagesimal = s.contains(':') && s.split(':').all(digits);
    WORDS.contains(&s.to_ascii_lowercase().as_str()) || date || sexagesimal
}

/// Stands in for each ambiguous string, so it can be quoted after writing.
fn mark_ambiguous(v: &Value, found: &mut Vec<String>) -> Value {
    let mut mark = |s: &str| {
        if ambiguous_in_yaml(s) {
            found.push(s.to_string());
            format!("deft-yaml-quote-{}-end", found.len() - 1)
        } else {
            s.to_string()
        }
    };
    match v {
        Value::String(s) => Value::String(mark(s)),
        Value::Array(a) => Value::Array(a.iter().map(|x| mark_ambiguous(x, found)).collect()),
        Value::Object(o) => {
            let mut out = Map::new();
            for (k, x) in o {
                let key = if ambiguous_in_yaml(k) {
                    found.push(k.clone());
                    format!("deft-yaml-quote-{}-end", found.len() - 1)
                } else {
                    k.clone()
                };
                out.insert(key, mark_ambiguous(x, found));
            }
            Value::Object(out)
        }
        other => other.clone(),
    }
}

fn to_yaml(v: &Value) -> std::result::Result<String, serde_yaml::Error> {
    let mut found = Vec::new();
    let marked = mark_ambiguous(v, &mut found);
    let mut text = serde_yaml::to_string(&marked)?;
    for (n, s) in found.iter().enumerate().rev() {
        text = text.replace(&format!("deft-yaml-quote-{n}-end"), &format!("'{s}'"));
    }
    Ok(text)
}

/// Check a document against a JSON Schema. Returns the problems found.
pub fn validate_schema(value: &Value, schema: &Value) -> Result<Vec<String>> {
    let mut schemas = boon::Schemas::new();
    let mut compiler = boon::Compiler::new();
    let url = "schema.json";
    let bad_schema = |e: boon::CompileError| err(ErrorCode::BadValue, format!("bad schema: {e}"));
    compiler
        .add_resource(url, schema.clone())
        .map_err(bad_schema)?;
    let index = compiler.compile(url, &mut schemas).map_err(bad_schema)?;
    Ok(match schemas.validate(value, index) {
        Ok(()) => vec![],
        Err(e) => format!("{e:#}")
            .lines()
            .map(|l| l.trim().to_string())
            .collect(),
    })
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
