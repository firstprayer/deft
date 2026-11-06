//! Structured data capabilities: read and write JSON, JSON Lines, YAML, TOML
//! and XML through one value model, query with jq syntax, merge and diff.

use deft_core::error::{ErrorCode, Result, err};
use deft_core::format::Format;
use serde_json::{Map, Value, json};

mod query;
mod xml;

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

fn parse_yaml(bytes: &[u8]) -> Result<Value> {
    use serde::Deserialize;
    let mut docs = Vec::new();
    for doc in serde_yaml::Deserializer::from_slice(bytes) {
        let mut v = serde_yaml::Value::deserialize(doc).map_err(|e| bad(Format::Yaml, e))?;
        v.apply_merge().map_err(|e| bad(Format::Yaml, e))?;
        docs.push(from_yaml(v));
    }
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
        Format::Yaml => serde_yaml::to_string(&v).map_err(|e| unable(&e))?,
        Format::Toml => {
            if !v.is_object() {
                return Err(err(
                    ErrorCode::InputUnsupported,
                    "TOML needs a table at the top; this document is a list or a single value",
                )
                .hint("wrap it first: deft data query <file> --expr '{items: .}'"));
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

/// One difference between two documents.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub path: String,
    pub kind: &'static str,
    pub before: Option<Value>,
    pub after: Option<Value>,
}

impl Change {
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("path".into(), json!(self.path));
        m.insert("kind".into(), json!(self.kind));
        if let Some(b) = &self.before {
            m.insert("before".into(), b.clone());
        }
        if let Some(a) = &self.after {
            m.insert("after".into(), a.clone());
        }
        Value::Object(m)
    }
}

/// Structural differences, with paths in jq syntax such as `.a.b[2]`.
pub fn diff(a: &Value, b: &Value) -> Vec<Change> {
    let mut out = Vec::new();
    walk_diff("", a, b, &mut out);
    out
}

fn key_path(base: &str, key: &str) -> String {
    let plain = !key.is_empty()
        && key.chars().all(|c| c.is_alphanumeric() || c == '_')
        && !key.starts_with(|c: char| c.is_ascii_digit());
    if plain {
        format!("{base}.{key}")
    } else {
        format!("{base}[{key:?}]")
    }
}

fn walk_diff(path: &str, a: &Value, b: &Value, out: &mut Vec<Change>) {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (k, va) in x {
                let p = key_path(path, k);
                match y.get(k) {
                    Some(vb) => walk_diff(&p, va, vb, out),
                    None => out.push(Change {
                        path: p,
                        kind: "removed",
                        before: Some(va.clone()),
                        after: None,
                    }),
                }
            }
            for (k, vb) in y {
                if !x.contains_key(k) {
                    out.push(Change {
                        path: key_path(path, k),
                        kind: "added",
                        before: None,
                        after: Some(vb.clone()),
                    });
                }
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            for i in 0..x.len().max(y.len()) {
                let p = format!("{path}[{i}]");
                match (x.get(i), y.get(i)) {
                    (Some(va), Some(vb)) => walk_diff(&p, va, vb, out),
                    (Some(va), None) => out.push(Change {
                        path: p,
                        kind: "removed",
                        before: Some(va.clone()),
                        after: None,
                    }),
                    (None, Some(vb)) => out.push(Change {
                        path: p,
                        kind: "added",
                        before: None,
                        after: Some(vb.clone()),
                    }),
                    (None, None) => {}
                }
            }
        }
        _ if a != b => out.push(Change {
            path: if path.is_empty() {
                ".".into()
            } else {
                path.into()
            },
            kind: "changed",
            before: Some(a.clone()),
            after: Some(b.clone()),
        }),
        _ => {}
    }
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
