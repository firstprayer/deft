//! XML to and from the JSON value model. Attributes are `@name`, text next to
//! child elements is `#text`, and repeated elements become arrays.

use serde_json::{Map, Value};

pub fn parse(text: &str) -> Result<Value, String> {
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    let doc = roxmltree::Document::parse_with_options(text, options).map_err(|e| e.to_string())?;
    let root = doc.root_element();
    let mut top = Map::new();
    top.insert(root.tag_name().name().to_string(), element(root));
    Ok(Value::Object(top))
}

fn element(node: roxmltree::Node) -> Value {
    let mut map = Map::new();
    for a in node.attributes() {
        map.insert(
            format!("@{}", a.name()),
            Value::String(a.value().to_string()),
        );
    }
    let mut text = String::new();
    for child in node.children() {
        if child.is_element() {
            let name = child.tag_name().name().to_string();
            let value = element(child);
            match map.get_mut(&name) {
                Some(Value::Array(list)) => list.push(value),
                Some(first) => {
                    let old = first.take();
                    *first = Value::Array(vec![old, value]);
                }
                None => {
                    map.insert(name, value);
                }
            }
        } else if child.is_text() {
            text.push_str(child.text().unwrap_or_default());
        }
    }
    let text = text.trim();
    if map.is_empty() {
        return if text.is_empty() {
            Value::Null
        } else {
            Value::String(text.to_string())
        };
    }
    if !text.is_empty() {
        map.insert("#text".into(), Value::String(text.to_string()));
    }
    Value::Object(map)
}

fn escape(s: &str, out: &mut String) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            c => out.push(c),
        }
    }
}

/// A key as an XML name: anything else becomes `_`.
fn name(key: &str) -> String {
    let mut s: String = key
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '_' | '-' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if !s.starts_with(|c: char| c.is_alphabetic() || c == '_') {
        s.insert(0, '_');
    }
    s
}

fn scalar(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn write(tag: &str, v: &Value, depth: usize, out: &mut String) {
    if let Value::Array(items) = v {
        for item in items {
            write(tag, item, depth, out);
        }
        return;
    }
    let pad = "  ".repeat(depth);
    let tag = name(tag);
    out.push_str(&format!("{pad}<{tag}"));
    let Value::Object(map) = v else {
        let body = scalar(v);
        if body.is_empty() {
            out.push_str("/>\n");
        } else {
            out.push('>');
            escape(&body, out);
            out.push_str(&format!("</{tag}>\n"));
        }
        return;
    };
    for (k, val) in map {
        if let Some(attr) = k.strip_prefix('@') {
            out.push_str(&format!(" {}=\"", name(attr)));
            escape(&scalar(val), out);
            out.push('"');
        }
    }
    let children: Vec<_> = map
        .iter()
        .filter(|(k, _)| !k.starts_with('@') && k.as_str() != "#text")
        .collect();
    let text = map.get("#text").map(scalar).unwrap_or_default();
    if children.is_empty() && text.is_empty() {
        out.push_str("/>\n");
        return;
    }
    out.push('>');
    escape(&text, out);
    if children.is_empty() {
        out.push_str(&format!("</{tag}>\n"));
        return;
    }
    out.push('\n');
    for (k, val) in children {
        write(k, val, depth + 1, out);
    }
    out.push_str(&format!("{pad}</{tag}>\n"));
}

pub fn render(v: &Value) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    match v {
        Value::Object(m) if m.len() == 1 && !m.values().any(Value::is_array) => {
            let (k, val) = m.iter().next().expect("one key");
            write(k, val, 0, &mut out);
        }
        Value::Array(items) => {
            let mut wrapped = Map::new();
            wrapped.insert("item".into(), Value::Array(items.clone()));
            write("root", &Value::Object(wrapped), 0, &mut out);
        }
        other => write("root", other, 0, &mut out),
    }
    out
}
