//! JSON Schema validation without the network. Real schemas (SchemaStore's
//! package.json, Cargo.toml, pyproject.toml) refer to other schemas by URL and
//! use JavaScript regular expressions with look-ahead. deft never fetches, so
//! a reference to another schema is left unchecked and reported; a pattern
//! the validator's regex engine cannot compile is checked with fancy-regex.

use deft_core::error::{ErrorCode, Result, err};
use serde_json::Value;
use std::collections::BTreeSet;

/// The outcome of a schema check.
#[derive(Debug, Default)]
pub struct Checked {
    pub problems: Vec<String>,
    /// Schemas referred to by URL that were not checked.
    pub unchecked: Vec<String>,
}

/// Keywords whose values are data, not schemas.
const DATA: &[&str] = &["const", "enum", "default", "examples", "example"];

/// Draft metaschemas the validator carries itself.
fn known(url: &str) -> bool {
    let u = url.trim_end_matches('#');
    u.starts_with("http://json-schema.org/") || u.starts_with("https://json-schema.org/")
}

fn without_fragment(u: &url::Url) -> String {
    let mut u = u.clone();
    u.set_fragment(None);
    u.to_string()
}

/// Every `$id` of the schema, resolved, so that a reference to one of them
/// stays inside the document.
fn ids(v: &Value, base: &url::Url, out: &mut BTreeSet<String>) {
    match v {
        Value::Object(m) => {
            let mut base = base.clone();
            for key in ["$id", "id"] {
                if let Some(Value::String(id)) = m.get(key)
                    && let Ok(u) = base.join(id)
                {
                    out.insert(without_fragment(&u));
                    base = u;
                }
            }
            for (k, x) in m {
                if !DATA.contains(&k.as_str()) {
                    ids(x, &base, out);
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|x| ids(x, base, out)),
        _ => {}
    }
}

/// Drop references to schemas that are neither in this document, nor on
/// disk next to it, nor a draft metaschema. Returns the URLs dropped.
fn drop_remote(
    v: &mut Value,
    base: &url::Url,
    local: &BTreeSet<String>,
    out: &mut BTreeSet<String>,
) {
    match v {
        Value::Object(m) => {
            let mut base = base.clone();
            for key in ["$id", "id"] {
                if let Some(Value::String(id)) = m.get(key)
                    && let Ok(u) = base.join(id)
                {
                    base = u;
                }
            }
            for key in ["$ref", "$dynamicRef", "$recursiveRef"] {
                if let Some(Value::String(r)) = m.get(key)
                    && let Ok(u) = base.join(r)
                {
                    let doc = without_fragment(&u);
                    let remote = matches!(u.scheme(), "http" | "https")
                        && !local.contains(&doc)
                        && !known(&doc);
                    if remote {
                        out.insert(doc);
                        m.remove(key);
                    }
                }
            }
            for (k, x) in m.iter_mut() {
                if !DATA.contains(&k.as_str()) {
                    drop_remote(x, &base, local, out);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(|x| drop_remote(x, base, local, out)),
        _ => {}
    }
}

/// Whether the validator's own engine takes a pattern.
fn plain_regex(p: &str) -> bool {
    regex::Regex::new(p).is_ok()
}

/// Every string and every object key of the instance: the only texts a
/// pattern is ever tested against.
fn texts(v: &Value, strings: &mut BTreeSet<String>, keys: &mut BTreeSet<String>) {
    match v {
        Value::String(s) => {
            strings.insert(s.clone());
        }
        Value::Array(a) => a.iter().for_each(|x| texts(x, strings, keys)),
        Value::Object(m) => {
            for (k, x) in m {
                keys.insert(k.clone());
                texts(x, strings, keys);
            }
        }
        _ => {}
    }
}

/// A pattern the regex crate cannot compile (a look-ahead, a back-reference)
/// becomes the list of the instance's own texts that it matches, which
/// gives the same answer for this instance.
fn exact_for(pattern: &str, candidates: &BTreeSet<String>) -> Option<String> {
    let re = fancy_regex::Regex::new(pattern).ok()?;
    let hits: Vec<String> = candidates
        .iter()
        .filter(|s| re.is_match(s).unwrap_or(false))
        .map(|s| regex::escape(s))
        .collect();
    Some(if hits.is_empty() {
        // Matches nothing.
        "[^\\s\\S]".to_string()
    } else {
        format!("^(?:{})$", hits.join("|"))
    })
}

fn rewrite_patterns(v: &mut Value, strings: &BTreeSet<String>, keys: &BTreeSet<String>) {
    match v {
        Value::Object(m) => {
            if let Some(Value::String(p)) = m.get("pattern")
                && !plain_regex(p)
                && let Some(exact) = exact_for(p, strings)
            {
                m.insert("pattern".into(), Value::String(exact));
            }
            if let Some(Value::Object(pp)) = m.get_mut("patternProperties") {
                let old = std::mem::take(pp);
                for (p, sub) in old {
                    let p = if plain_regex(&p) {
                        p
                    } else {
                        exact_for(&p, keys).unwrap_or(p)
                    };
                    pp.insert(p, sub);
                }
            }
            for (k, x) in m.iter_mut() {
                if !DATA.contains(&k.as_str()) {
                    rewrite_patterns(x, strings, keys);
                }
            }
        }
        Value::Array(a) => a
            .iter_mut()
            .for_each(|x| rewrite_patterns(x, strings, keys)),
        _ => {}
    }
}

/// Check a document against a JSON Schema. `location` is the schema file,
/// so that references to files beside it resolve.
pub fn validate(
    value: &Value,
    schema: &Value,
    location: Option<&std::path::Path>,
) -> Result<Checked> {
    let base = location
        .and_then(|p| std::path::absolute(p).ok())
        .and_then(|p| url::Url::from_file_path(p).ok())
        .unwrap_or_else(|| url::Url::parse("file:///schema.json").expect("valid"));
    let mut schema = schema.clone();
    let mut local = BTreeSet::new();
    local.insert(without_fragment(&base));
    ids(&schema, &base, &mut local);
    let mut remote = BTreeSet::new();
    drop_remote(&mut schema, &base, &local, &mut remote);
    let (mut strings, mut keys) = (BTreeSet::new(), BTreeSet::new());
    texts(value, &mut strings, &mut keys);
    // `propertyNames` applies `pattern` to keys too.
    strings.extend(keys.iter().cloned());
    rewrite_patterns(&mut schema, &strings, &keys);

    let mut schemas = boon::Schemas::new();
    let mut compiler = boon::Compiler::new();
    let bad_schema = |e: boon::CompileError| err(ErrorCode::BadValue, format!("bad schema: {e:#}"));
    compiler
        .add_resource(base.as_str(), schema)
        .map_err(bad_schema)?;
    let index = compiler
        .compile(base.as_str(), &mut schemas)
        .map_err(bad_schema)?;
    let problems = match schemas.validate(value, index) {
        Ok(()) => vec![],
        Err(e) => format!("{e:#}")
            .lines()
            .map(|l| l.trim().replace("at ''", "at '' (the top level)"))
            .collect(),
    };
    Ok(Checked {
        problems,
        unchecked: remote.into_iter().collect(),
    })
}

#[cfg(test)]
#[path = "schema_tests.rs"]
mod tests;
