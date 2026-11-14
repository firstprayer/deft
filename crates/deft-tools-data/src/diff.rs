//! Structural differences between two documents. Lists are matched the way
//! a person reads them: items with the same identity (a `name`, an `id`, a
//! package's name and version, a Kubernetes object's kind and name) are the
//! same item wherever they stand, and other lists are aligned on equal items,
//! so one inserted item is one change rather than a shift of every later one.

use serde_json::{Map, Value, json};

/// One difference between two documents.
#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    /// Where, in jq syntax. A list index is the item's place in the second
    /// document, or in the first for a removed item.
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
    walk("", a, b, &mut out);
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

fn index_path(base: &str, i: usize) -> String {
    // jq needs the dot when the document itself is a list.
    let base = if base.is_empty() { "." } else { base };
    format!("{base}[{i}]")
}

fn change(
    path: String,
    kind: &'static str,
    before: Option<&Value>,
    after: Option<&Value>,
) -> Change {
    Change {
        path: if path.is_empty() { ".".into() } else { path },
        kind,
        before: before.cloned(),
        after: after.cloned(),
    }
}

fn walk(path: &str, a: &Value, b: &Value, out: &mut Vec<Change>) {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (k, va) in x {
                let p = key_path(path, k);
                match y.get(k) {
                    Some(vb) => walk(&p, va, vb, out),
                    None => out.push(change(p, "removed", Some(va), None)),
                }
            }
            for (k, vb) in y {
                if !x.contains_key(k) {
                    out.push(change(key_path(path, k), "added", None, Some(vb)));
                }
            }
        }
        (Value::Array(x), Value::Array(y)) => list(path, x, y, out),
        _ if a != b => out.push(change(path.into(), "changed", Some(a), Some(b))),
        _ => {}
    }
}

/// Fields that name an item on their own, most telling first. A dotted
/// name looks inside.
const IDENTITY: &[&str] = &[
    "name",
    "id",
    "key",
    "uid",
    "uuid",
    "metadata.name",
    "package",
    "path",
    "url",
    "title",
    "slug",
    "target",
    "host",
    "file",
    "src",
    "source",
    "repo",
    "label",
];

/// Fields that tell items apart only together with one of the above, such
/// as a package's version or a Kubernetes object's kind.
const QUALIFIERS: &[&str] = &["version", "kind", "type", "metadata.namespace", "group"];

fn field<'v>(v: &'v Value, name: &str) -> Option<&'v Value> {
    let mut cur = v;
    for part in name.split('.') {
        cur = cur.as_object()?.get(part)?;
    }
    match cur {
        Value::String(_) | Value::Number(_) | Value::Bool(_) => Some(cur),
        _ => None,
    }
}

fn identity(v: &Value, fields: &[&str]) -> Option<String> {
    let parts: Option<Vec<String>> = fields
        .iter()
        .map(|f| field(v, f).map(Value::to_string))
        .collect();
    parts.map(|p| p.join("\u{1}"))
}

/// The fewest identity fields (one or two) that every item has and that tell
/// the items of each list apart.
fn identity_fields(x: &[Value], y: &[Value]) -> Option<Vec<&'static str>> {
    if x.is_empty() || y.is_empty() || !x.iter().chain(y).all(Value::is_object) {
        return None;
    }
    let all = |f: &&str| x.iter().chain(y).all(|v| field(v, f).is_some());
    let strong: Vec<&'static str> = IDENTITY.iter().copied().filter(all).collect();
    let weak: Vec<&'static str> = QUALIFIERS.iter().copied().filter(all).collect();
    let unique = |fields: &[&str]| {
        [x, y].iter().all(|list| {
            let mut seen = std::collections::HashSet::new();
            list.iter()
                .all(|v| identity(v, fields).is_some_and(|id| seen.insert(id)))
        })
    };
    if let Some(f) = strong.iter().find(|f| unique(&[f])) {
        return Some(vec![f]);
    }
    for (i, f) in strong.iter().enumerate() {
        for g in strong[i + 1..].iter().chain(&weak) {
            if unique(&[f, g]) {
                return Some(vec![f, g]);
            }
        }
    }
    None
}

fn list(path: &str, x: &[Value], y: &[Value], out: &mut Vec<Change>) {
    if let Some(fields) = identity_fields(x, y) {
        let ids_x: Vec<String> = x.iter().filter_map(|v| identity(v, &fields)).collect();
        let ids_y: Vec<String> = y.iter().filter_map(|v| identity(v, &fields)).collect();
        // Items that kept their identity are compared field by field.
        let at_y: std::collections::HashMap<&str, usize> = ids_y
            .iter()
            .enumerate()
            .map(|(j, id)| (id.as_str(), j))
            .collect();
        let shared = ids_x
            .iter()
            .filter(|id| at_y.contains_key(id.as_str()))
            .count();
        // Two lists of mostly different items are better aligned by content.
        if shared * 2 >= x.len().min(y.len()) {
            let mut used = vec![false; y.len()];
            for (i, id) in ids_x.iter().enumerate() {
                match at_y.get(id.as_str()) {
                    Some(&j) => {
                        used[j] = true;
                        walk(&index_path(path, j), &x[i], &y[j], out);
                    }
                    None => out.push(change(index_path(path, i), "removed", Some(&x[i]), None)),
                }
            }
            for (j, v) in y.iter().enumerate() {
                if !used[j] {
                    out.push(change(index_path(path, j), "added", None, Some(v)));
                }
            }
            return;
        }
    }
    aligned(path, x, y, out);
}

/// Align two lists on equal items (longest common subsequence). Between
/// equal runs, items are paired in order and compared; the rest are added
/// or removed.
fn aligned(path: &str, x: &[Value], y: &[Value], out: &mut Vec<Change>) {
    let (n, m) = (x.len(), y.len());
    // Keep the table small: trim the equal ends first.
    let mut lo = 0;
    while lo < n && lo < m && x[lo] == y[lo] {
        lo += 1;
    }
    let mut hi = 0;
    while hi < n - lo && hi < m - lo && x[n - 1 - hi] == y[m - 1 - hi] {
        hi += 1;
    }
    let xs = &x[lo..n - hi];
    let ys = &y[lo..m - hi];
    let pairs = if xs.len().saturating_mul(ys.len()) <= 4_000_000 {
        lcs(xs, ys)
    } else {
        Vec::new()
    };
    let (mut i, mut j) = (0, 0);
    let gap = |i0: usize, i1: usize, j0: usize, j1: usize, out: &mut Vec<Change>| {
        let k = (i1 - i0).min(j1 - j0);
        for t in 0..k {
            walk(
                &index_path(path, lo + j0 + t),
                &xs[i0 + t],
                &ys[j0 + t],
                out,
            );
        }
        for (t, v) in xs.iter().enumerate().take(i1).skip(i0 + k) {
            out.push(change(index_path(path, lo + t), "removed", Some(v), None));
        }
        for (t, v) in ys.iter().enumerate().take(j1).skip(j0 + k) {
            out.push(change(index_path(path, lo + t), "added", None, Some(v)));
        }
    };
    for (pi, pj) in pairs {
        gap(i, pi, j, pj, out);
        i = pi + 1;
        j = pj + 1;
    }
    gap(i, xs.len(), j, ys.len(), out);
}

/// Index pairs of a longest common subsequence of equal items.
fn lcs(x: &[Value], y: &[Value]) -> Vec<(usize, usize)> {
    let (n, m) = (x.len(), y.len());
    let mut t = vec![0u32; (n + 1) * (m + 1)];
    let at = |i: usize, j: usize| i * (m + 1) + j;
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            t[at(i, j)] = if x[i] == y[j] {
                t[at(i + 1, j + 1)] + 1
            } else {
                t[at(i + 1, j)].max(t[at(i, j + 1)])
            };
        }
    }
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < n && j < m {
        if x[i] == y[j] {
            out.push((i, j));
            i += 1;
            j += 1;
        } else if t[at(i + 1, j)] >= t[at(i, j + 1)] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

#[cfg(test)]
#[path = "diff_tests.rs"]
mod tests;
