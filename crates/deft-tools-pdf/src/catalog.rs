//! Document-level entries of the catalog that removing pages or merging must
//! carry over: attachments, page labels and layers.

use crate::{deref, num};
use lopdf::{Dictionary, Document, Object, ObjectId, dictionary};
use std::collections::HashSet;

/// The entries of a name or number tree, in tree order.
fn tree_entries(doc: &Document, node: &Object, key: &[u8], out: &mut Vec<(Object, Object)>) {
    fn walk(
        doc: &Document,
        node: &Object,
        key: &[u8],
        out: &mut Vec<(Object, Object)>,
        seen: &mut HashSet<ObjectId>,
    ) {
        if let Ok(id) = node.as_reference()
            && !seen.insert(id)
        {
            return;
        }
        let Ok(dict) = deref(doc, node).as_dict() else {
            return;
        };
        if let Ok(pairs) = dict
            .get(key)
            .map(|o| deref(doc, o))
            .and_then(Object::as_array)
        {
            for pair in pairs.chunks_exact(2) {
                out.push((deref(doc, &pair[0]).clone(), pair[1].clone()));
            }
        }
        if let Ok(kids) = dict
            .get(b"Kids")
            .map(|o| deref(doc, o))
            .and_then(Object::as_array)
        {
            for kid in kids {
                walk(doc, kid, key, out, seen);
            }
        }
    }
    walk(doc, node, key, out, &mut HashSet::new());
}

fn catalog_entry<'a>(doc: &'a Document, key: &[u8]) -> Option<&'a Object> {
    doc.catalog().ok()?.get(key).ok()
}

/// A catalog entry for changing, wherever it is kept: an indirect dictionary
/// is changed in place, a direct one inside the catalog.
fn catalog_dict_mut<'a>(doc: &'a mut Document, key: &[u8]) -> Option<&'a mut Dictionary> {
    match catalog_entry(doc, key)?.as_reference() {
        Ok(id) => doc.get_dictionary_mut(id).ok(),
        Err(_) => doc
            .catalog_mut()
            .ok()?
            .get_mut(key)
            .ok()?
            .as_dict_mut()
            .ok(),
    }
}

// ------------------------------------------------------------------ attachments

/// Name trees that name pages or destinations. The rest (attachments,
/// document scripts) belong to the whole document.
const PAGE_TREES: &[&[u8]] = &[b"Dests", b"Pages", b"Templates"];

/// After pages are removed: drop the name trees that point at pages and keep
/// the others, such as attachments. Named destinations must be inlined first.
pub(crate) fn keep_document_names(doc: &mut Document) {
    let Some(names) = catalog_dict_mut(doc, b"Names") else {
        if let Ok(c) = doc.catalog_mut() {
            c.remove(b"Names");
        }
        return;
    };
    for key in PAGE_TREES {
        names.remove(key);
    }
    if names.is_empty()
        && let Ok(c) = doc.catalog_mut()
    {
        c.remove(b"Names");
    }
}

/// The attachments of a document as (name, file specification) pairs.
pub(crate) fn attachments(doc: &Document) -> Vec<(Object, Object)> {
    let mut out = Vec::new();
    let tree = catalog_entry(doc, b"Names")
        .and_then(|o| deref(doc, o).as_dict().ok())
        .and_then(|n| n.get(b"EmbeddedFiles").ok());
    if let Some(tree) = tree {
        tree_entries(doc, tree, b"Names", &mut out);
    }
    out
}

/// Add attachments to a document, renaming any whose name is taken. Their
/// objects must already be part of `doc`.
pub(crate) fn append_attachments(doc: &mut Document, added: Vec<(Object, Object)>) {
    if added.is_empty() {
        return;
    }
    let mut all = attachments(doc);
    let mut taken: HashSet<Vec<u8>> = all
        .iter()
        .filter_map(|(k, _)| k.as_str().ok().map(<[u8]>::to_vec))
        .collect();
    for (name, spec) in added {
        let base = name.as_str().map(<[u8]>::to_vec).unwrap_or_default();
        let mut unique = base.clone();
        let mut n = 2;
        while !taken.insert(unique.clone()) {
            let suffix = format!(" ({n})");
            // A name in UTF-16 takes its suffix in UTF-16 too.
            let suffix: Vec<u8> = if base.starts_with(&[0xFE, 0xFF]) {
                suffix.encode_utf16().flat_map(u16::to_be_bytes).collect()
            } else {
                suffix.into_bytes()
            };
            unique = [base.as_slice(), &suffix].concat();
            n += 1;
        }
        all.push((Object::string_literal(unique), spec));
    }
    // A name tree is sorted by its keys.
    all.sort_by(|a, b| a.0.as_str().unwrap_or(b"").cmp(b.0.as_str().unwrap_or(b"")));
    let flat: Vec<Object> = all.into_iter().flat_map(|(k, v)| [k, v]).collect();
    let tree = doc.add_object(dictionary! { "Names" => flat });
    if catalog_dict_mut(doc, b"Names").is_none()
        && let Ok(c) = doc.catalog_mut()
    {
        c.set("Names", Dictionary::new());
    }
    if let Some(names) = catalog_dict_mut(doc, b"Names") {
        names.set("EmbeddedFiles", tree);
    }
}

// ------------------------------------------------------------------ page labels

/// How one page is numbered: style (D, R, r, A, a or none), prefix, number.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Label {
    style: Option<Vec<u8>>,
    prefix: Option<Vec<u8>>,
    number: i64,
}

impl Label {
    fn decimal(number: i64) -> Label {
        Label {
            style: Some(b"D".to_vec()),
            prefix: None,
            number,
        }
    }

    fn follows(&self, before: &Label) -> bool {
        self.style == before.style
            && self.prefix == before.prefix
            && self.number == before.number + 1
    }
}

/// The label of every page, or `None` when the document has no labels.
pub(crate) fn page_labels(doc: &Document, pages: usize) -> Option<Vec<Label>> {
    let tree = catalog_entry(doc, b"PageLabels")?;
    let mut ranges = Vec::new();
    tree_entries(doc, tree, b"Nums", &mut ranges);
    let mut ranges: Vec<(usize, Label)> = ranges
        .into_iter()
        .filter_map(|(k, v)| {
            let start = num(&k)?;
            let d = deref(doc, &v).as_dict().ok()?;
            let label = Label {
                style: d
                    .get(b"S")
                    .and_then(Object::as_name)
                    .ok()
                    .map(<[u8]>::to_vec),
                prefix: d
                    .get(b"P")
                    .ok()
                    .and_then(|o| deref(doc, o).as_str().ok())
                    .map(<[u8]>::to_vec),
                number: d
                    .get(b"St")
                    .ok()
                    .and_then(|o| num(deref(doc, o)))
                    .map_or(1, |n| n as i64),
            };
            (start >= 0.0).then_some((start as usize, label))
        })
        .collect();
    if ranges.is_empty() {
        return None;
    }
    ranges.sort_by_key(|r| r.0);
    let mut out = Vec::with_capacity(pages);
    for page in 0..pages {
        let label = match ranges.iter().rev().find(|r| r.0 <= page) {
            Some((start, l)) => Label {
                number: l.number + (page - start) as i64,
                ..l.clone()
            },
            // Pages before the first range have no label: number them.
            None => Label::decimal(page as i64 + 1),
        };
        out.push(label);
    }
    Some(out)
}

/// Write page labels, one range for each run of pages numbered in sequence.
pub(crate) fn set_page_labels(doc: &mut Document, labels: &[Label]) {
    let mut nums = Vec::new();
    for (i, label) in labels.iter().enumerate() {
        if i > 0 && label.follows(&labels[i - 1]) {
            continue;
        }
        let mut d = Dictionary::new();
        if let Some(s) = &label.style {
            d.set("S", Object::Name(s.clone()));
        }
        if let Some(p) = &label.prefix {
            d.set("P", Object::string_literal(p.clone()));
        }
        if label.number != 1 {
            d.set("St", label.number);
        }
        nums.push(Object::Integer(i as i64));
        nums.push(Object::Dictionary(d));
    }
    let tree = doc.add_object(dictionary! { "Nums" => nums });
    if let Ok(c) = doc.catalog_mut() {
        c.set("PageLabels", tree);
    }
}

/// Labels for documents joined in order: each keeps its own; pages of a
/// document without labels are numbered as they now stand.
pub(crate) fn joined_labels(parts: &[(usize, Option<Vec<Label>>)]) -> Option<Vec<Label>> {
    if parts.iter().all(|p| p.1.is_none()) {
        return None;
    }
    let mut out = Vec::new();
    for (pages, labels) in parts {
        match labels {
            Some(l) => out.extend(l.iter().cloned()),
            None => {
                let first = out.len() as i64 + 1;
                out.extend((0..*pages as i64).map(|i| Label::decimal(first + i)));
            }
        }
    }
    Some(out)
}

// ------------------------------------------------------------------ layers

/// The layers (optional content) of a document, as a direct dictionary.
pub(crate) fn layers(doc: &Document) -> Option<Dictionary> {
    let props = deref(doc, catalog_entry(doc, b"OCProperties")?)
        .as_dict()
        .ok()?;
    let mut props = props.clone();
    if let Ok(d) = props.get(b"D").map(|o| deref(doc, o).clone()) {
        props.set("D", d);
    }
    Some(props)
}

/// Add the layers of another document. Its objects must already be part of
/// `doc`. The lists of groups and of the default view's settings are joined.
pub(crate) fn append_layers(doc: &mut Document, added: Dictionary) {
    let Some(mut props) = layers(doc) else {
        if let Ok(c) = doc.catalog_mut() {
            c.set("OCProperties", added);
        }
        return;
    };
    let array = |doc: &Document, d: &Dictionary, k: &[u8]| -> Vec<Object> {
        d.get(k)
            .ok()
            .map(|o| deref(doc, o))
            .and_then(|o| o.as_array().ok())
            .cloned()
            .unwrap_or_default()
    };
    let mut groups = array(doc, &props, b"OCGs");
    groups.extend(array(doc, &added, b"OCGs"));
    props.set("OCGs", groups);
    let view = |p: &Dictionary| p.get(b"D").ok().and_then(|o| o.as_dict().ok()).cloned();
    if let (Some(mut d), Some(mut other)) = (view(&props), view(&added)) {
        // Groups are on unless listed otherwise; the base state says which.
        let base = |d: &Dictionary| {
            d.get(b"BaseState")
                .and_then(Object::as_name)
                .unwrap_or(b"ON")
                == b"ON"
        };
        if base(&d) != base(&other) {
            let (from, to): (&[u8], &[u8]) = if base(&other) {
                (b"OFF", b"ON")
            } else {
                (b"ON", b"OFF")
            };
            let listed = array(doc, &other, from);
            let mut set = array(doc, &other, to);
            set.extend(
                array(doc, &added, b"OCGs")
                    .into_iter()
                    .filter(|g| !listed.contains(g)),
            );
            other.set(to.to_vec(), set);
        }
        for key in [&b"ON"[..], b"OFF", b"Order", b"RBGroups", b"Locked"] {
            let mut joined = array(doc, &d, key);
            joined.extend(array(doc, &other, key));
            if !joined.is_empty() {
                d.set(key.to_vec(), joined);
            }
        }
        props.set("D", d);
    }
    if let Ok(c) = doc.catalog_mut() {
        c.set("OCProperties", props);
    }
}

#[cfg(test)]
#[path = "catalog_tests.rs"]
mod tests;
