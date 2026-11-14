//! XML to and from the JSON value model. Attributes are `@name`, repeated
//! elements become arrays, and an element that mixes text with child
//! elements keeps both, in order, as a `#content` list. Apple property lists
//! read as what they mean: a `<dict>` is an object of its `<key>`s.

use serde_json::{Map, Value};

pub fn parse(text: &str) -> Result<Value, String> {
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    let doc = roxmltree::Document::parse_with_options(text, options).map_err(|e| e.to_string())?;
    let root = doc.root_element();
    let plist = root.tag_name().name() == "plist";
    let mut top = Map::new();
    top.insert(root.tag_name().name().to_string(), element(root, plist));
    Ok(Value::Object(top))
}

/// What the value model cannot hold of this document.
pub fn lost(text: &str) -> Vec<String> {
    let options = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..Default::default()
    };
    let Ok(doc) = roxmltree::Document::parse_with_options(text, options) else {
        return vec![];
    };
    let elements = || doc.descendants().filter(roxmltree::Node::is_element);
    let clash = elements().any(|e| {
        let mut seen = std::collections::HashMap::new();
        e.children().filter(roxmltree::Node::is_element).any(|c| {
            let name = c.tag_name();
            *seen.entry(name.name()).or_insert(name.namespace()) != name.namespace()
        })
    });
    let mut out = Vec::new();
    if clash {
        out.push("elements from two namespaces share a name and were merged".to_string());
    }
    out
}

fn filled(n: &roxmltree::Node) -> bool {
    n.is_text() && !n.text().unwrap_or("").trim().is_empty()
}

/// A property-list value: dict, array, string, integer, real, true, false,
/// date or data.
fn plist_value(node: roxmltree::Node) -> Option<Value> {
    let text = || node.text().unwrap_or("").to_string();
    let number = |t: String| {
        // Keep the text when the number would not spell it the same way.
        match t.trim().parse::<serde_json::Number>() {
            Ok(n) if n.to_string() == t.trim() => Value::Number(n),
            _ => Value::String(t),
        }
    };
    Some(match node.tag_name().name() {
        "dict" => {
            let mut map = Map::new();
            let mut key: Option<String> = None;
            for c in node.children().filter(roxmltree::Node::is_element) {
                if c.tag_name().name() == "key" {
                    key = Some(c.text().unwrap_or("").to_string());
                } else {
                    let k = key.take()?;
                    map.insert(k, plist_value(c)?);
                }
            }
            if key.is_some() {
                return None;
            }
            Value::Object(map)
        }
        "array" => Value::Array(
            node.children()
                .filter(roxmltree::Node::is_element)
                .map(plist_value)
                .collect::<Option<Vec<_>>>()?,
        ),
        "string" | "date" | "data" => Value::String(text()),
        "integer" | "real" => number(text()),
        "true" => Value::Bool(true),
        "false" => Value::Bool(false),
        _ => return None,
    })
}

fn element(node: roxmltree::Node, plist: bool) -> Value {
    if plist
        && matches!(node.tag_name().name(), "dict" | "array")
        && let Some(v) = plist_value(node)
    {
        return v;
    }
    let mut map = Map::new();
    for a in node.attributes() {
        map.insert(
            format!("@{}", a.name()),
            Value::String(a.value().to_string()),
        );
    }
    let mixed = node.children().any(|c| filled(&c)) && node.children().any(|c| c.is_element());
    if mixed {
        // Text and elements in their order, so a paragraph still reads right.
        let kids: Vec<roxmltree::Node> = node.children().collect();
        let last = kids.len().saturating_sub(1);
        let content = kids
            .iter()
            .enumerate()
            .filter_map(|(i, &c)| {
                if c.is_element() {
                    let mut one = Map::new();
                    one.insert(c.tag_name().name().to_string(), element(c, plist));
                    Some(Value::Object(one))
                } else if c.is_text() {
                    // A space between two inline elements is part of the text;
                    // only the indentation before the first and after the last goes.
                    let t = c.text().unwrap_or_default();
                    let edge = i == 0 || i == last;
                    (!(edge && t.trim().is_empty())).then(|| Value::String(t.to_string()))
                } else {
                    None
                }
            })
            .collect();
        map.insert("#content".into(), Value::Array(content));
        return Value::Object(map);
    }
    let mut text = String::new();
    for child in node.children() {
        if child.is_element() {
            let name = child.tag_name().name().to_string();
            let value = element(child, plist);
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

/// Whether a key can be an element name as it is.
fn xml_name(key: &str) -> bool {
    let mut chars = key.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

fn scalar(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

struct Out {
    text: String,
    /// Keys written as `<item key="...">` because they are not XML names.
    renamed: usize,
}

impl Out {
    /// The start of an element for a key: its own name when it is an XML
    /// name, else `item` with the key as an attribute.
    fn open(&mut self, pad: &str, key: &str) -> String {
        if xml_name(key) {
            self.text.push_str(&format!("{pad}<{key}"));
            key.to_string()
        } else {
            self.renamed += 1;
            self.text.push_str(&format!("{pad}<item key=\""));
            escape(key, &mut self.text);
            self.text.push('"');
            "item".to_string()
        }
    }

    fn write(&mut self, key: &str, v: &Value, depth: usize) {
        let pad = "  ".repeat(depth);
        if let Value::Array(items) = v {
            if items.is_empty() {
                // An empty list keeps its key.
                self.open(&pad, key);
                self.text.push_str("/>\n");
            }
            for item in items {
                self.write(key, item, depth);
            }
            return;
        }
        let tag = self.open(&pad, key);
        let Value::Object(map) = v else {
            let body = scalar(v);
            if body.is_empty() {
                self.text.push_str("/>\n");
            } else {
                self.text.push('>');
                escape(&body, &mut self.text);
                self.text.push_str(&format!("</{tag}>\n"));
            }
            return;
        };
        for (k, val) in map {
            if let Some(attr) = k.strip_prefix('@')
                && xml_name(attr)
                && !val.is_object()
                && !val.is_array()
            {
                self.text.push_str(&format!(" {attr}=\""));
                escape(&scalar(val), &mut self.text);
                self.text.push('"');
            }
        }
        let is_attr = |k: &str, val: &Value| {
            k.strip_prefix('@')
                .is_some_and(|a| xml_name(a) && !val.is_object() && !val.is_array())
        };
        let children: Vec<_> = map
            .iter()
            .filter(|(k, val)| {
                !is_attr(k, val) && k.as_str() != "#text" && k.as_str() != "#content"
            })
            .collect();
        let text = map.get("#text").map(scalar).unwrap_or_default();
        let content = map.get("#content").and_then(Value::as_array);
        if children.is_empty() && text.is_empty() && content.is_none() {
            self.text.push_str("/>\n");
            return;
        }
        self.text.push('>');
        if let Some(content) = content {
            // Mixed content goes back on one line, in its order.
            for part in content {
                match part {
                    Value::Object(one) => {
                        for (k, val) in one {
                            let mut inner = Out {
                                text: String::new(),
                                renamed: 0,
                            };
                            inner.write(k, val, 0);
                            self.renamed += inner.renamed;
                            self.text.push_str(inner.text.trim_end_matches('\n'));
                        }
                    }
                    other => escape(&scalar(other), &mut self.text),
                }
            }
            if children.is_empty() {
                self.text.push_str(&format!("</{tag}>\n"));
                return;
            }
        }
        escape(&text, &mut self.text);
        if children.is_empty() {
            self.text.push_str(&format!("</{tag}>\n"));
            return;
        }
        self.text.push('\n');
        for (k, val) in children {
            self.write(k, val, depth + 1);
        }
        self.text.push_str(&format!("{pad}</{tag}>\n"));
    }
}

/// Write XML. Returns the text and how many keys were not XML names.
pub fn render(v: &Value) -> (String, usize) {
    let mut out = Out {
        text: String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n"),
        renamed: 0,
    };
    match v {
        Value::Object(m) if m.len() == 1 && !m.values().any(Value::is_array) => {
            let (k, val) = m.iter().next().expect("one key");
            out.write(k, val, 0);
        }
        Value::Array(items) => {
            let mut wrapped = Map::new();
            wrapped.insert("item".into(), Value::Array(items.clone()));
            out.write("root", &Value::Object(wrapped), 0);
        }
        other => out.write("root", other, 0),
    }
    (out.text, out.renamed)
}
