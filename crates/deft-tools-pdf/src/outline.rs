//! Bookmarks and named destinations.

use crate::deref;
use lopdf::{Dictionary, Document, Object, ObjectId, dictionary};
use std::collections::{HashMap, HashSet};

/// A destination as an array, from either form a name can point at.
fn explicit(doc: &Document, value: &Object) -> Option<Object> {
    match deref(doc, value) {
        Object::Array(a) => Some(Object::Array(a.clone())),
        Object::Dictionary(d) => explicit(doc, d.get(b"D").ok()?),
        _ => None,
    }
}

fn name_tree(
    doc: &Document,
    node: &Dictionary,
    out: &mut HashMap<Vec<u8>, Object>,
    seen: &mut HashSet<ObjectId>,
) {
    if let Some(pairs) = node
        .get(b"Names")
        .ok()
        .and_then(|o| deref(doc, o).as_array().ok())
    {
        for pair in pairs.chunks_exact(2) {
            if let (Ok(name), Some(dest)) = (pair[0].as_str(), explicit(doc, &pair[1])) {
                out.insert(name.to_vec(), dest);
            }
        }
    }
    let kids = node
        .get(b"Kids")
        .ok()
        .and_then(|o| deref(doc, o).as_array().ok());
    for kid in kids.into_iter().flatten() {
        if let Ok(id) = kid.as_reference()
            && seen.insert(id)
            && let Ok(d) = doc.get_dictionary(id)
        {
            name_tree(doc, d, out, seen);
        }
    }
}

fn named_dests(doc: &Document) -> HashMap<Vec<u8>, Object> {
    let mut out = HashMap::new();
    let Ok(catalog) = doc.catalog() else {
        return out;
    };
    if let Some(dests) = catalog
        .get(b"Dests")
        .ok()
        .and_then(|o| deref(doc, o).as_dict().ok())
    {
        for (name, value) in dests.iter() {
            if let Some(dest) = explicit(doc, value) {
                out.insert(name.clone(), dest);
            }
        }
    }
    if let Some(tree) = catalog
        .get(b"Names")
        .ok()
        .and_then(|o| deref(doc, o).as_dict().ok())
        .and_then(|names| names.get(b"Dests").ok())
        .and_then(|o| deref(doc, o).as_dict().ok())
    {
        name_tree(doc, tree, &mut out, &mut HashSet::new());
    }
    out
}

fn replace_name(slot: &mut Object, names: &HashMap<Vec<u8>, Object>) {
    let found = match slot {
        Object::Name(n) => names.get(n.as_slice()),
        Object::String(s, _) => names.get(s.as_slice()),
        _ => None,
    };
    if let Some(dest) = found {
        *slot = dest.clone();
    }
}

fn inline(o: &mut Object, names: &HashMap<Vec<u8>, Object>) {
    match o {
        Object::Array(a) => a.iter_mut().for_each(|x| inline(x, names)),
        Object::Dictionary(d) => {
            if let Ok(dest) = d.get_mut(b"Dest") {
                replace_name(dest, names);
            }
            let go_to = d.get(b"S").and_then(Object::as_name).ok() == Some(b"GoTo");
            if go_to && let Ok(dest) = d.get_mut(b"D") {
                replace_name(dest, names);
            }
            d.iter_mut().for_each(|(_, x)| inline(x, names));
        }
        _ => {}
    }
}

/// Write named destinations out where they are used. Names live in the catalog,
/// which does not survive removing pages or merging.
pub(crate) fn inline_named_dests(doc: &mut Document) {
    let names = named_dests(doc);
    if !names.is_empty() {
        doc.objects.values_mut().for_each(|o| inline(o, &names));
    }
}

/// The page a link or bookmark goes to.
pub(crate) fn dest_page(doc: &Document, item: &Dictionary) -> Option<ObjectId> {
    let dest = item.get(b"Dest").ok().or_else(|| {
        let action = deref(doc, item.get(b"A").ok()?).as_dict().ok()?;
        action.get(b"D").ok()
    })?;
    deref(doc, dest)
        .as_array()
        .ok()?
        .first()?
        .as_reference()
        .ok()
}

/// One bookmark and those under it.
struct Item {
    id: ObjectId,
    kids: Vec<Item>,
}

fn items(doc: &Document, parent: ObjectId, seen: &mut HashSet<ObjectId>) -> Vec<Item> {
    let mut out = Vec::new();
    let first = |d: &Dictionary| d.get(b"First").and_then(Object::as_reference).ok();
    let mut at = doc.get_dictionary(parent).ok().and_then(first);
    while let Some(id) = at {
        if !seen.insert(id) {
            break;
        }
        let Ok(dict) = doc.get_dictionary(id) else {
            break;
        };
        at = dict.get(b"Next").and_then(Object::as_reference).ok();
        out.push(Item {
            id,
            kids: items(doc, id, seen),
        });
    }
    out
}

fn is_closed(doc: &Document, id: ObjectId) -> bool {
    doc.get_dictionary(id)
        .ok()
        .and_then(|d| d.get(b"Count").ok())
        .and_then(|o| o.as_i64().ok())
        .is_some_and(|n| n < 0)
}

/// Write the links of a level of bookmarks, returning how many rows it shows.
fn relink(doc: &mut Document, parent: ObjectId, level: &[Item]) -> i64 {
    let mut shown = 0;
    for (i, item) in level.iter().enumerate() {
        let closed = is_closed(doc, item.id);
        let below = relink(doc, item.id, &item.kids);
        shown += 1 + if closed { 0 } else { below };
        let Ok(d) = doc.get_dictionary_mut(item.id) else {
            continue;
        };
        d.set("Parent", Object::Reference(parent));
        for (key, other) in [
            (&b"Prev"[..], i.checked_sub(1).map(|p| &level[p])),
            (b"Next", level.get(i + 1)),
        ] {
            match other {
                Some(o) => d.set(key.to_vec(), Object::Reference(o.id)),
                None => {
                    d.remove(key);
                }
            }
        }
        if !item.kids.is_empty() {
            d.set("Count", if closed { -below } else { below });
        }
    }
    if let Ok(d) = doc.get_dictionary_mut(parent) {
        match (level.first(), level.last()) {
            (Some(first), Some(last)) => {
                d.set("First", Object::Reference(first.id));
                d.set("Last", Object::Reference(last.id));
            }
            _ => {
                for key in [&b"First"[..], b"Last", b"Count"] {
                    d.remove(key);
                }
            }
        }
    }
    shown
}

fn root(doc: &Document) -> Option<ObjectId> {
    let catalog = doc.catalog().ok()?;
    catalog.get(b"Outlines").and_then(Object::as_reference).ok()
}

fn filtered(doc: &mut Document, level: Vec<Item>, keep: &HashSet<ObjectId>) -> Vec<Item> {
    let mut out = Vec::new();
    for item in level {
        let had_kids = !item.kids.is_empty();
        let kids = filtered(doc, item.kids, keep);
        let Ok(dict) = doc.get_dictionary(item.id) else {
            continue;
        };
        let gone = dest_page(doc, dict).is_some_and(|p| !keep.contains(&p));
        let links = dict.has(b"Dest") || dict.has(b"A");
        if gone && let Ok(d) = doc.get_dictionary_mut(item.id) {
            d.remove(b"Dest");
            d.remove(b"A");
        }
        // A heading stays for what is left under it.
        if kids.is_empty() && (gone || (had_kids && !links)) {
            continue;
        }
        out.push(Item { id: item.id, kids });
    }
    out
}

/// Keep the bookmarks that still have somewhere to go.
pub(crate) fn keep_pages(doc: &mut Document, keep: &HashSet<ObjectId>) {
    let Some(top) = root(doc) else {
        return;
    };
    let level = items(doc, top, &mut HashSet::new());
    let level = filtered(doc, level, keep);
    let shown = relink(doc, top, &level);
    if level.is_empty() {
        if let Ok(catalog) = doc.catalog_mut() {
            catalog.remove(b"Outlines");
        }
    } else if let Ok(d) = doc.get_dictionary_mut(top) {
        d.set("Count", shown);
    }
}

/// Put the bookmarks under `other` after those of `doc`. Its objects must
/// already be part of `doc`.
pub(crate) fn append(doc: &mut Document, other: ObjectId) {
    let mut seen = HashSet::new();
    let added = items(doc, other, &mut seen);
    if added.is_empty() {
        return;
    }
    let top = match root(doc) {
        Some(id) => id,
        None => {
            let id = doc.add_object(dictionary! { "Type" => "Outlines" });
            if let Ok(catalog) = doc.catalog_mut() {
                catalog.set("Outlines", Object::Reference(id));
            }
            id
        }
    };
    let mut level = items(doc, top, &mut seen);
    level.extend(added);
    let shown = relink(doc, top, &level);
    if let Ok(d) = doc.get_dictionary_mut(top) {
        d.set("Count", shown);
    }
}

/// The bookmark root of a catalog.
pub(crate) fn root_of(catalog: &Dictionary) -> Option<ObjectId> {
    catalog.get(b"Outlines").and_then(Object::as_reference).ok()
}

/// Every bookmark, flat.
pub(crate) fn all_items(doc: &Document) -> Vec<ObjectId> {
    fn walk(level: &[Item], out: &mut Vec<ObjectId>) {
        for item in level {
            out.push(item.id);
            walk(&item.kids, out);
        }
    }
    let mut out = Vec::new();
    if let Some(top) = root(doc) {
        walk(&items(doc, top, &mut HashSet::new()), &mut out);
    }
    out
}
