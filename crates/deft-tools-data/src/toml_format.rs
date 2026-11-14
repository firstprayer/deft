//! Tidy a TOML file in place of its own text, so comments, blank lines,
//! number spellings (nan, inf, 0x1F, 1_000) and string styles stay as written.
//! What changes: indentation before keys and headers, spacing around `=`,
//! trailing spaces, runs of blank lines, and with `sort` the key order.

use toml_edit::{DocumentMut, Item, RawString, Table};

/// The comment and blank lines of a decor prefix, without indentation:
/// at most one blank line in a row, each comment on its own line.
fn tidy_prefix(raw: Option<&RawString>) -> String {
    let text = raw.and_then(|r| r.as_str()).unwrap_or("");
    let mut lines: Vec<&str> = text.split('\n').collect();
    // The last piece is the indentation of the key or header itself.
    lines.pop();
    let mut out = String::new();
    let mut blank = false;
    for line in lines {
        let t = line.trim();
        if t.is_empty() {
            blank = true;
            continue;
        }
        if blank {
            out.push('\n');
            blank = false;
        }
        out.push_str(t);
        out.push('\n');
    }
    if blank {
        out.push('\n');
    }
    out
}

/// A trailing comment, kept one space after the value.
fn tidy_suffix(raw: Option<&RawString>) -> String {
    let t = raw.and_then(|r| r.as_str()).unwrap_or("").trim();
    if t.starts_with('#') {
        format!(" {t}")
    } else {
        String::new()
    }
}

/// Sort the keys of inline tables, at any depth.
fn sort_inline(v: &mut toml_edit::Value) {
    match v {
        toml_edit::Value::InlineTable(t) => {
            t.sort_values();
            t.iter_mut().for_each(|(_, x)| sort_inline(x));
            // Inline tables hold no comments; respace them after the move.
            t.fmt();
        }
        toml_edit::Value::Array(a) => a.iter_mut().for_each(sort_inline),
        _ => {}
    }
}

fn tidy_table(t: &mut Table, sort: bool) {
    if sort {
        t.sort_values_by(|a, _, b, _| a.get().cmp(b.get()));
        // Dotted keys (`a.b = 1`) stand among the plain keys; where the table
        // also has [sections], they become sections too, so that all of its
        // tables follow in sorted order.
        let sections = t
            .iter()
            .any(|(_, i)| matches!(i, Item::Table(s) if !s.is_dotted()) || i.is_array_of_tables());
        if sections {
            for (mut key, item) in t.iter_mut() {
                if let Item::Table(sub) = item
                    && sub.is_dotted()
                {
                    let prefix = tidy_prefix(key.leaf_decor().prefix());
                    key.leaf_decor_mut().clear();
                    sub.set_dotted(false);
                    sub.set_implicit(false);
                    sub.decor_mut().set_prefix(prefix);
                }
            }
        }
    }
    for (mut key, item) in t.iter_mut() {
        match item {
            Item::Value(v) => {
                if sort {
                    sort_inline(v);
                }
                let prefix = tidy_prefix(key.leaf_decor().prefix());
                key.leaf_decor_mut().set_prefix(prefix);
                key.leaf_decor_mut().set_suffix(" ");
                let suffix = tidy_suffix(v.decor().suffix());
                v.decor_mut().set_prefix(" ");
                v.decor_mut().set_suffix(suffix);
            }
            Item::Table(sub) if sub.is_dotted() => {
                let prefix = tidy_prefix(key.leaf_decor().prefix());
                key.leaf_decor_mut().set_prefix(prefix);
                tidy_table(sub, sort);
            }
            Item::Table(sub) => {
                tidy_header(sub);
                tidy_table(sub, sort);
            }
            Item::ArrayOfTables(a) => {
                for sub in a.iter_mut() {
                    tidy_header(sub);
                    tidy_table(sub, sort);
                }
            }
            Item::None => {}
        }
    }
}

/// A `[header]` starts at the line start, after one blank line.
fn tidy_header(t: &mut Table) {
    let prefix = tidy_prefix(t.decor().prefix());
    let prefix = if prefix.starts_with('\n') {
        prefix
    } else {
        format!("\n{prefix}")
    };
    let suffix = tidy_suffix(t.decor().suffix());
    t.decor_mut().set_prefix(prefix);
    t.decor_mut().set_suffix(suffix);
}

/// Give headers positions in sorted key order, depth first.
fn place(t: &mut Table, next: &mut usize) {
    for (_, item) in t.iter_mut() {
        match item {
            Item::Table(sub) => {
                if !sub.is_dotted() {
                    sub.set_position(*next);
                    *next += 1;
                }
                place(sub, next);
            }
            Item::ArrayOfTables(a) => {
                for sub in a.iter_mut() {
                    sub.set_position(*next);
                    *next += 1;
                    place(sub, next);
                }
            }
            _ => {}
        }
    }
}

/// Tidy TOML text. With `sort`, keys are sorted within each table and the
/// tables follow in sorted order; comments move with their keys.
pub fn format(text: &str, sort: bool) -> Result<String, String> {
    let mut doc: DocumentMut = text
        .parse()
        .map_err(|e: toml_edit::TomlError| e.to_string())?;
    tidy_table(doc.as_table_mut(), sort);
    if sort {
        place(doc.as_table_mut(), &mut 1);
    }
    let raw = RawString::from(format!("{}\n", doc.trailing().as_str().unwrap_or("")));
    let trailing = tidy_prefix(Some(&raw));
    doc.set_trailing(trailing.trim_end_matches('\n').to_string());
    let mut out = doc.to_string();
    // The first header needs no blank line above it.
    while out.starts_with('\n') {
        out.remove(0);
    }
    let out = out.trim_end().to_string() + "\n";
    // Check the result reads as the same data; refuse rather than change it.
    let before: toml::Value = toml::from_str(text).map_err(|e| e.to_string())?;
    let after: toml::Value =
        toml::from_str(&out).map_err(|e| format!("tidied text does not parse: {e}"))?;
    if !same(&before, &after) {
        return Err("tidying would change the data".into());
    }
    Ok(out)
}

/// Equal data; nan equals nan here.
fn same(a: &toml::Value, b: &toml::Value) -> bool {
    use toml::Value as T;
    match (a, b) {
        (T::Float(x), T::Float(y)) => x == y || (x.is_nan() && y.is_nan()),
        (T::Array(x), T::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(a, b)| same(a, b))
        }
        (T::Table(x), T::Table(y)) => {
            x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| same(v, w)))
        }
        _ => a == b,
    }
}

#[cfg(test)]
#[path = "toml_format_tests.rs"]
mod tests;
