//! PDF capabilities built on lopdf (structure) and hayro (rendering).
//! Pure functions over bytes and documents; no CLI knowledge.

use deft_core::error::{ErrorCode, Result, err};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};
use std::collections::{HashMap, HashSet};

mod catalog;
pub mod compare;
pub mod extract;
mod fonts;
pub mod forms;
pub mod layout;
pub mod meta;
mod outline;
pub mod pages;
pub mod text;
mod xmp;

pub use lopdf;

// ------------------------------------------------------------------ load / save

/// Parse a PDF. Encrypted files open with an empty user password when
/// possible; otherwise `password` is required. A protected file keeps its
/// encryption state, so `meta::save_protected` can protect the result again.
pub fn load(bytes: &[u8], password: Option<&str>) -> Result<Document> {
    let mut doc = match attachments_only(bytes) {
        Some(doc) => doc,
        None => load_decrypted(bytes, password)?,
    };
    recover_packed_objects(&mut doc);
    let pages = doc.get_pages();
    if pages.is_empty() {
        return Err(err(ErrorCode::InputCorrupt, "the PDF has no pages"));
    }
    // lopdf keeps a page it cannot read as a stand-in, which would be written
    // out as a null page. Refuse rather than drop a page.
    if let Some((n, _)) = pages
        .iter()
        .find(|(_, id)| doc.get_dictionary(**id).is_err())
    {
        return Err(err(
            ErrorCode::InputCorrupt,
            format!("page {n} cannot be read; the file is damaged"),
        ));
    }
    // lopdf's text extraction misses resources inherited from parent nodes.
    flatten_inheritance(&mut doc);
    Ok(doc)
}

/// Read the objects of an object stream that lopdf skipped. Its parser stops
/// at a comment before an object, which qpdf and others write, so the page
/// stored there would be lost.
fn recover_packed_objects(doc: &mut Document) {
    let mut wanted = Vec::new();
    for o in doc.objects.values() {
        references(o, &mut wanted);
    }
    if wanted.iter().all(|id| doc.objects.contains_key(id)) {
        return;
    }
    let mut found = Vec::new();
    for (&(container, _), obj) in &doc.objects {
        let Ok(stream) = obj.as_stream() else {
            continue;
        };
        if !stream.dict.has_type(b"ObjStm") {
            continue;
        }
        let Ok(plain) = stream.get_plain_content() else {
            continue;
        };
        let mut cleaned = stream.clone();
        cleaned.dict.remove(b"Filter");
        cleaned.dict.remove(b"DecodeParms");
        cleaned.set_content(blank_comments(&plain));
        let Ok(objects) = lopdf::ObjectStream::new(&cleaned) else {
            continue;
        };
        for (id, o) in objects.objects {
            // Only objects this container still holds, by the cross-reference
            // table; an older copy must not shadow a newer one.
            let current = match doc.reference_table.get(id.0) {
                None => true,
                Some(lopdf::xref::XrefEntry::Compressed { container: c, .. }) => *c == container,
                Some(_) => false,
            };
            if current && !doc.objects.contains_key(&id) {
                found.push((id, o));
            }
        }
    }
    for (id, o) in found {
        doc.max_id = doc.max_id.max(id.0);
        doc.objects.insert(id, o);
    }
}

/// Replace comments with spaces, keeping every offset in place.
fn blank_comments(data: &[u8]) -> Vec<u8> {
    let mut out = data.to_vec();
    let (mut depth, mut i) = (0usize, 0);
    while i < out.len() {
        match out[i] {
            b'\\' if depth > 0 => i += 1,
            b'(' => depth += 1,
            b')' if depth > 0 => depth -= 1,
            b'%' if depth == 0 => {
                while i < out.len() && !matches!(out[i], b'\r' | b'\n') {
                    out[i] = b' ';
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }
    out
}

fn load_decrypted(bytes: &[u8], password: Option<&str>) -> Result<Document> {
    let options = match password {
        Some(p) => lopdf::LoadOptions::with_password(p),
        None => lopdf::LoadOptions::default(),
    };
    let doc = Document::load_mem_with_options(bytes, options).map_err(|e| match e {
        lopdf::Error::InvalidPassword | lopdf::Error::Decryption(_) => {
            encrypted_error(password.is_some())
        }
        other => err(
            ErrorCode::InputCorrupt,
            format!("not a readable PDF: {other}"),
        ),
    })?;
    // lopdf leaves /Encrypt in place when no password opened the file.
    if doc.trailer.get(b"Encrypt").is_ok() {
        return Err(encrypted_error(password.is_some()));
    }
    Ok(doc)
}

/// A file where only the attachments are encrypted (Acrobat's "encrypt only
/// file attachments"): strings and streams use the Identity filter, so the
/// document reads without a password. lopdf would ask for one, or decrypt the
/// plain text into noise. Read it as it is and keep /Encrypt for the
/// attachments, which stay encrypted.
fn attachments_only(bytes: &[u8]) -> Option<Document> {
    if !bytes.windows(4).any(|w| w == b"/EFF") {
        return None;
    }
    // Hide the trailer key so lopdf reads the objects as written.
    let mut hidden = bytes.to_vec();
    let mut i = 0;
    while let Some(at) = hidden[i..].windows(8).position(|w| w == b"/Encrypt") {
        let end = i + at + 8;
        if hidden.get(end).is_some_and(|b| b.is_ascii_whitespace()) {
            hidden[end - 1] = b'x';
        }
        i = end;
    }
    let mut doc = Document::load_mem(&hidden).ok()?;
    let key = doc.trailer.remove(b"Encrypx")?;
    let dict = deref(&doc, &key).as_dict().ok()?;
    let version = dict.get(b"V").ok().and_then(num).unwrap_or(0.0);
    let identity = |k: &[u8]| {
        dict.get(k)
            .ok()
            .and_then(|o| o.as_name().ok())
            .is_none_or(|n| n == b"Identity")
    };
    if version < 4.0 || !identity(b"StmF") || !identity(b"StrF") {
        return None;
    }
    doc.trailer.set("Encrypt", key);
    Some(doc)
}

fn encrypted_error(had_password: bool) -> deft_core::error::DeftError {
    if had_password {
        err(ErrorCode::InputEncrypted, "wrong password")
    } else {
        err(ErrorCode::InputEncrypted, "the PDF is password-protected")
            .hint("add --password <password>")
    }
}

/// Serialise with object and cross-reference streams (smallest output).
pub fn save(doc: &mut Document) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    doc.save_modern(&mut out)
        .map_err(|e| err(ErrorCode::Internal, format!("writing PDF: {e}")))?;
    Ok(out)
}

pub fn page_ids(doc: &Document) -> Vec<ObjectId> {
    doc.get_pages().into_values().collect()
}

// ------------------------------------------------------------------ geometry

pub(crate) fn num(o: &Object) -> Option<f64> {
    match o {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(*r as f64),
        _ => None,
    }
}

pub(crate) fn deref<'a>(doc: &'a Document, o: &'a Object) -> &'a Object {
    doc.dereference(o).map(|(_, o)| o).unwrap_or(o)
}

pub(crate) fn rect_of(doc: &Document, o: &Object) -> Option<[f64; 4]> {
    let arr = deref(doc, o).as_array().ok()?;
    if arr.len() != 4 {
        return None;
    }
    let v: Vec<f64> = arr
        .iter()
        .map(|x| num(deref(doc, x)))
        .collect::<Option<_>>()?;
    Some([
        v[0].min(v[2]),
        v[1].min(v[3]),
        v[0].max(v[2]),
        v[1].max(v[3]),
    ])
}

/// Look up a page attribute, following /Parent for inherited keys.
pub(crate) fn inherited<'a>(doc: &'a Document, page: ObjectId, key: &[u8]) -> Option<&'a Object> {
    let mut id = page;
    for _ in 0..64 {
        let dict = doc.get_dictionary(id).ok()?;
        if let Ok(v) = dict.get(key) {
            return Some(v);
        }
        id = dict.get(b"Parent").and_then(Object::as_reference).ok()?;
    }
    None
}

/// Visible box (CropBox, else MediaBox) in user space.
pub fn page_box(doc: &Document, page: ObjectId) -> [f64; 4] {
    inherited(doc, page, b"CropBox")
        .and_then(|o| rect_of(doc, o))
        .or_else(|| inherited(doc, page, b"MediaBox").and_then(|o| rect_of(doc, o)))
        .unwrap_or([0.0, 0.0, 612.0, 792.0])
}

pub fn page_rotation(doc: &Document, page: ObjectId) -> i64 {
    inherited(doc, page, b"Rotate")
        .and_then(|o| num(deref(doc, o)))
        .map(|r| (r as i64).rem_euclid(360))
        .unwrap_or(0)
}

/// Page size as displayed (width, height in points), honouring /Rotate.
pub fn display_size(doc: &Document, page: ObjectId) -> (f64, f64) {
    let b = page_box(doc, page);
    let (w, h) = (b[2] - b[0], b[3] - b[1]);
    if page_rotation(doc, page) % 180 == 90 {
        (h, w)
    } else {
        (w, h)
    }
}

/// Matrix mapping display space (origin bottom-left of the page as shown,
/// x right, y up) to user space, so stamps appear upright on rotated pages.
pub(crate) fn display_matrix(doc: &Document, page: ObjectId) -> [f64; 6] {
    let [x0, y0, x1, y1] = page_box(doc, page);
    let (w, h) = (x1 - x0, y1 - y0);
    match page_rotation(doc, page) {
        90 => [0.0, 1.0, -1.0, 0.0, x0 + w, y0],
        180 => [-1.0, 0.0, 0.0, -1.0, x0 + w, y0 + h],
        270 => [0.0, -1.0, 1.0, 0.0, x0, y0 + h],
        _ => [1.0, 0.0, 0.0, 1.0, x0, y0],
    }
}

pub const PAPER: &[(&str, f64, f64)] = &[
    ("a3", 841.89, 1190.55),
    ("a4", 595.28, 841.89),
    ("a5", 419.53, 595.28),
    ("letter", 612.0, 792.0),
    ("legal", 612.0, 1008.0),
];

/// Common page size name for (w, h) in points, if any.
pub fn paper_name(w: f64, h: f64) -> Option<&'static str> {
    let (a, b) = (w.min(h), w.max(h));
    PAPER
        .iter()
        .find(|(_, pw, ph)| (a - pw).abs() < 3.0 && (b - ph).abs() < 3.0)
        .map(|p| p.0)
}

// ------------------------------------------------------------------ page tree

/// Copy inherited attributes onto each page so pages survive being moved to
/// a new page tree.
fn flatten_inheritance(doc: &mut Document) {
    for id in page_ids(doc) {
        let mut add = Vec::new();
        for key in [&b"Resources"[..], b"MediaBox", b"CropBox", b"Rotate"] {
            let has = doc.get_dictionary(id).map(|d| d.has(key)).unwrap_or(true);
            if !has && let Some(v) = inherited(doc, id, key) {
                add.push((key.to_vec(), v.clone()));
            }
        }
        if let Ok(d) = doc.get_dictionary_mut(id) {
            for (k, v) in add {
                d.set(k, v);
            }
        }
    }
}

/// Replace the page tree with a flat one listing `pages` in order. Pages
/// used more than once are cloned. Unreferenced objects are pruned.
fn set_page_tree(doc: &mut Document, pages: &[ObjectId]) -> Result<()> {
    let old_pages: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter(|(_, o)| o.as_dict().is_ok_and(|d| d.has_type(b"Pages")))
        .map(|(id, _)| *id)
        .collect();
    let tree_id = doc.new_object_id();
    let mut seen = HashSet::new();
    let mut kids = Vec::new();
    for &p in pages {
        let id = if seen.insert(p) {
            p
        } else {
            let copy = doc
                .get_dictionary(p)
                .map_err(|e| err(ErrorCode::InputCorrupt, e.to_string()))?
                .clone();
            doc.add_object(Object::Dictionary(copy))
        };
        if let Ok(d) = doc.get_dictionary_mut(id) {
            d.set("Parent", Object::Reference(tree_id));
        }
        kids.push(Object::Reference(id));
    }
    let count = kids.len() as i64;
    doc.objects.insert(
        tree_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }),
    );
    let no_catalog = || err(ErrorCode::InputCorrupt, "the PDF has no catalog");
    let root = doc
        .trailer
        .get(b"Root")
        .and_then(Object::as_reference)
        .map_err(|_| no_catalog())?;
    let catalog = doc.get_dictionary_mut(root).map_err(|_| no_catalog())?;
    catalog.set("Pages", Object::Reference(tree_id));
    catalog.remove(b"PageLabels"); // labels refer to the old page order
    for id in old_pages {
        doc.objects.remove(&id);
    }
    prune(doc);
    Ok(())
}

/// Every object an object refers to directly.
pub(crate) fn references(o: &Object, out: &mut Vec<ObjectId>) {
    match o {
        Object::Reference(id) => out.push(*id),
        Object::Array(a) => a.iter().for_each(|x| references(x, out)),
        Object::Dictionary(d) => d.iter().for_each(|(_, x)| references(x, out)),
        Object::Stream(s) => s.dict.iter().for_each(|(_, x)| references(x, out)),
        _ => {}
    }
}

/// Drop objects unreachable from the trailer. lopdf's `prune_objects` scans a
/// list once per object, which takes seconds on long documents.
pub(crate) fn prune(doc: &mut Document) {
    let refs = references;
    let mut stack = Vec::new();
    doc.trailer.iter().for_each(|(_, o)| refs(o, &mut stack));
    let mut seen = HashSet::new();
    while let Some(id) = stack.pop() {
        if seen.insert(id)
            && let Some(o) = doc.objects.get(&id)
        {
            refs(o, &mut stack);
        }
    }
    doc.objects.retain(|id, _| seen.contains(id));
}

/// After removing pages, cut what still points at them: bookmarks and fields on
/// those pages, the structure tree, named destinations and links. Otherwise the
/// removed pages stay reachable and their content stays in the file.
fn detach_removed_pages(doc: &mut Document, kept: &[ObjectId]) -> Result<()> {
    let keep: HashSet<ObjectId> = kept.iter().copied().collect();
    outline::inline_named_dests(doc);
    outline::keep_pages(doc, &keep);
    forms::keep_fields_on(doc, kept);
    if let Ok(catalog) = doc.catalog_mut() {
        for key in [&b"StructTreeRoot"[..], b"Dests", b"OpenAction"] {
            catalog.remove(key);
        }
    }
    catalog::keep_document_names(doc);
    let points_away = |doc: &Document, annot: &Dictionary| {
        outline::dest_page(doc, annot).is_some_and(|id| !keep.contains(&id))
    };
    for &page in kept {
        let annots: Vec<Object> = match doc
            .get_dictionary(page)
            .ok()
            .and_then(|d| d.get(b"Annots").ok())
        {
            Some(o) => deref(doc, o).as_array().cloned().unwrap_or_default(),
            None => continue,
        };
        let filtered: Vec<Object> = annots
            .iter()
            .filter(|a| {
                deref(doc, a)
                    .as_dict()
                    .map(|d| !points_away(doc, d))
                    .unwrap_or(true)
            })
            .cloned()
            .collect();
        if let Ok(d) = doc.get_dictionary_mut(page) {
            d.set("Annots", Object::Array(filtered));
        }
    }
    Ok(())
}

/// Keep (and reorder or repeat) pages by zero-based index.
pub fn select(doc: &mut Document, indexes: &[usize]) -> Result<()> {
    if indexes.is_empty() {
        return Err(err(
            ErrorCode::BadValue,
            "no pages left; a PDF needs at least one page",
        ));
    }
    flatten_inheritance(doc);
    let ids = page_ids(doc);
    let labels = catalog::page_labels(doc, ids.len());
    let picked: Vec<ObjectId> = indexes.iter().map(|&i| ids[i]).collect();
    let dropped = ids.iter().any(|id| !picked.contains(id));
    if dropped {
        detach_removed_pages(doc, &picked)?;
    }
    set_page_tree(doc, &picked)?;
    // Each page keeps the label it had.
    if let Some(labels) = labels {
        let kept: Vec<_> = indexes.iter().map(|&i| labels[i].clone()).collect();
        catalog::set_page_labels(doc, &kept);
    }
    Ok(())
}

/// Split into documents of the given pages (zero-based) each. Each part is
/// made from the objects its pages and the document level need, not from a
/// copy of the whole file, so a long document splits in time that grows with
/// its length.
pub fn split(doc: &mut Document, groups: &[Vec<usize>]) -> Result<Vec<Document>> {
    flatten_inheritance(doc);
    outline::inline_named_dests(doc);
    let ids = page_ids(doc);
    let pages: HashSet<ObjectId> = ids.iter().copied().collect();
    // What an object needs, not going on into pages, which every part picks
    // for itself, nor up to a parent.
    let reach = |doc: &Document, from: Vec<ObjectId>, seen: &mut HashSet<ObjectId>| {
        let mut stack = from;
        while let Some(id) = stack.pop() {
            if !seen.insert(id) {
                continue;
            }
            let Some(o) = doc.objects.get(&id) else {
                continue;
            };
            let mut next = Vec::new();
            match o {
                Object::Dictionary(d) => d
                    .iter()
                    .filter(|(k, _)| !matches!(k.as_slice(), b"Parent" | b"StructTreeRoot"))
                    .for_each(|(_, v)| references(v, &mut next)),
                other => references(other, &mut next),
            }
            stack.extend(next.into_iter().filter(|n| !pages.contains(n)));
        }
    };
    let mut shared = HashSet::new();
    let mut roots = Vec::new();
    doc.trailer
        .iter()
        .for_each(|(_, o)| references(o, &mut roots));
    reach(doc, roots, &mut shared);
    // The page tree's own nodes, so the part keeps every page's place.
    for o in doc
        .objects
        .iter()
        .filter(|(_, o)| o.as_dict().is_ok_and(|d| d.has_type(b"Pages")))
    {
        shared.insert(*o.0);
    }
    let objects = std::mem::take(&mut doc.objects);
    let table = std::mem::replace(
        &mut doc.reference_table,
        lopdf::xref::Xref::new(0, lopdf::xref::XrefType::CrossReferenceStream),
    );
    let frame = doc.clone();
    doc.objects = objects;
    doc.reference_table = table;
    let mut parts = Vec::with_capacity(groups.len());
    for group in groups {
        let mut keep = shared.clone();
        let picked: Vec<ObjectId> = group.iter().map(|&i| ids[i]).collect();
        reach(doc, picked, &mut keep);
        let mut part = frame.clone();
        part.objects = keep
            .iter()
            .filter_map(|id| Some((*id, doc.objects.get(id)?.clone())))
            .collect();
        // Pages of other parts are absent: lopdf keeps their places as
        // stand-ins, which select then drops.
        select_known(&mut part, &ids, group)?;
        parts.push(part);
    }
    Ok(parts)
}

/// `select` for a part whose other pages are absent; `ids` are all the
/// document's pages in order.
fn select_known(doc: &mut Document, ids: &[ObjectId], indexes: &[usize]) -> Result<()> {
    if indexes.is_empty() {
        return Err(err(
            ErrorCode::BadValue,
            "no pages left; a PDF needs at least one page",
        ));
    }
    let labels = catalog::page_labels(doc, ids.len());
    let picked: Vec<ObjectId> = indexes.iter().map(|&i| ids[i]).collect();
    detach_removed_pages(doc, &picked)?;
    set_page_tree(doc, &picked)?;
    if let Some(labels) = labels {
        let kept: Vec<_> = indexes.iter().map(|&i| labels[i].clone()).collect();
        catalog::set_page_labels(doc, &kept);
    }
    Ok(())
}

/// Add `offset` to every object number. lopdf's renumbering takes seconds on
/// long documents.
fn shift_ids(doc: &mut Document, offset: u32) {
    fn shift(o: &mut Object, offset: u32) {
        match o {
            Object::Reference(id) => id.0 += offset,
            Object::Array(a) => a.iter_mut().for_each(|x| shift(x, offset)),
            Object::Dictionary(d) => d.iter_mut().for_each(|(_, x)| shift(x, offset)),
            Object::Stream(s) => s.dict.iter_mut().for_each(|(_, x)| shift(x, offset)),
            _ => {}
        }
    }
    let objects = std::mem::take(&mut doc.objects);
    doc.objects = objects
        .into_iter()
        .map(|(id, mut o)| {
            shift(&mut o, offset);
            ((id.0 + offset, id.1), o)
        })
        .collect();
    doc.trailer.iter_mut().for_each(|(_, o)| shift(o, offset));
    doc.max_id += offset;
}

/// Join documents in order. The first document's catalog is kept, and the forms,
/// bookmarks, attachments, page labels and layers of the others join it.
pub fn merge(mut docs: Vec<Document>) -> Result<Document> {
    let mut base = docs.remove(0);
    flatten_inheritance(&mut base);
    let mut all = page_ids(&base);
    let mut labels = vec![(all.len(), catalog::page_labels(&base, all.len()))];
    for mut other in docs {
        // A protected input keeps the result protected. Older revisions derive
        // the key from the file ID, so the ID goes with the protection.
        if base.encryption_state.is_none()
            && let Some(state) = other.encryption_state.take()
        {
            base.encryption_state = Some(state);
            match other.trailer.get(b"ID").ok().cloned() {
                Some(id) => base.trailer.set("ID", id),
                None => {
                    base.trailer.remove(b"ID");
                }
            }
        }
        flatten_inheritance(&mut other);
        // Its names go with its catalog.
        outline::inline_named_dests(&mut other);
        shift_ids(&mut other, base.max_id);
        let pages = page_ids(&other);
        labels.push((pages.len(), catalog::page_labels(&other, pages.len())));
        let bookmarks = other.catalog().ok().and_then(outline::root_of);
        let form = forms::acro_form(&other).cloned();
        let attachments = catalog::attachments(&other);
        let layers = catalog::layers(&other);
        let skip: HashSet<ObjectId> = other
            .objects
            .iter()
            .filter(|(_, o)| {
                o.as_dict()
                    .is_ok_and(|d| d.has_type(b"Catalog") || d.has_type(b"Pages"))
            })
            .map(|(id, _)| *id)
            .collect();
        let max_id = other.max_id;
        for (id, obj) in other.objects {
            if !skip.contains(&id) {
                base.objects.insert(id, obj);
            }
        }
        base.max_id = base.max_id.max(max_id);
        all.extend(pages);
        if let Some(root) = bookmarks {
            outline::append(&mut base, root);
        }
        if let Some(form) = form {
            forms::append_form(&mut base, &form);
        }
        catalog::append_attachments(&mut base, attachments);
        if let Some(layers) = layers {
            catalog::append_layers(&mut base, layers);
        }
    }
    set_page_tree(&mut base, &all)?;
    if let Some(labels) = catalog::joined_labels(&labels) {
        catalog::set_page_labels(&mut base, &labels);
    }
    Ok(base)
}

/// Add `angle` (a multiple of 90) to the rotation of the given pages.
pub fn rotate(doc: &mut Document, indexes: &[usize], angle: i64) {
    let ids = page_ids(doc);
    for &i in indexes {
        let id = ids[i];
        let current = page_rotation(doc, id);
        if let Ok(d) = doc.get_dictionary_mut(id) {
            d.set("Rotate", (current + angle).rem_euclid(360));
        }
    }
}

// ------------------------------------------------------------------ info

pub struct Info {
    pub pages: usize,
    pub version: String,
    pub title: Option<String>,
    pub author: Option<String>,
    pub producer: Option<String>,
    pub width: f64,
    pub height: f64,
    pub paper: Option<&'static str>,
    pub images: usize,
    pub fonts: usize,
    pub forms: bool,
}

pub(crate) fn text_string(doc: &Document, o: &Object) -> Option<String> {
    let bytes = deref(doc, o).as_str().ok()?;
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_be_bytes(*c))
            .collect();
        return Some(String::from_utf16_lossy(&units));
    }
    Some(bytes.iter().map(|&b| b as char).collect())
}

pub fn info(doc: &Document) -> Info {
    let ids = page_ids(doc);
    let (width, height) = display_size(doc, ids[0]);
    let meta = doc
        .trailer
        .get(b"Info")
        .ok()
        .map(|o| deref(doc, o))
        .and_then(|o| o.as_dict().ok());
    let field = |k: &[u8]| {
        meta.and_then(|m| m.get(k).ok())
            .and_then(|o| text_string(doc, o))
            .filter(|s| !s.trim().is_empty())
    };
    // The pictures the pages show, as pdf extract-images saves them: soft
    // masks and stencil masks of a picture are part of it, and images no
    // page uses are left out.
    let mut seen = HashSet::new();
    let mut used = Vec::new();
    for &page in &ids {
        if let Ok(dict) = doc.get_dictionary(page)
            && let Some(res) = extract::resources(doc, dict)
        {
            extract::images_of(doc, res, &mut seen, &mut used);
        }
    }
    let images = used.len();
    let fonts = doc
        .objects
        .values()
        .filter(|o| o.as_dict().is_ok_and(|d| d.has_type(b"Font")))
        .count();
    Info {
        pages: ids.len(),
        version: doc.version.clone(),
        title: field(b"Title"),
        author: field(b"Author"),
        producer: field(b"Producer"),
        width: (width * 10.0).round() / 10.0,
        height: (height * 10.0).round() / 10.0,
        paper: paper_name(width, height),
        images,
        fonts,
        // An empty /AcroForm is common in books and scans; only fields count.
        forms: !forms::fields(doc).is_empty(),
    }
}

/// Plain text of the given pages (zero-based), pages separated by form feeds.
pub fn extract_text(doc: &Document, indexes: &[usize]) -> Result<String> {
    let mut out = String::new();
    for (n, page) in text::reader(doc)?.page_texts(indexes).iter().enumerate() {
        if n > 0 {
            out.push_str("\x0c\n");
        }
        out.push_str(page);
        out.push('\n');
    }
    Ok(out)
}

/// Join the lines of extracted text into paragraphs. A blank line, or a line
/// clearly shorter than the longest, ends a paragraph.
pub fn reflow(text: &str) -> Vec<String> {
    let lines: Vec<&str> = text
        .lines()
        .map(|l| l.trim_matches(|c: char| c.is_whitespace() || c == '\x0c'))
        .collect();
    let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
    let mut out = Vec::new();
    let mut para = String::new();
    for line in lines {
        if line.is_empty() {
            if !para.is_empty() {
                out.push(std::mem::take(&mut para));
            }
            continue;
        }
        if para.ends_with('-') && line.starts_with(char::is_lowercase) {
            para.pop();
        } else if !para.is_empty() {
            para.push(' ');
        }
        para.push_str(line);
        if line.chars().count() * 4 < longest * 3 {
            out.push(std::mem::take(&mut para));
        }
    }
    if !para.is_empty() {
        out.push(para);
    }
    out
}

// ------------------------------------------------------------------ stamping

/// Helvetica advance widths (1/1000 em) for ASCII 32..=126.
const HELVETICA: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667,
    611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500,
    222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];

/// Encode text for a WinAnsi Helvetica font. Errors on characters outside it.
pub fn win_ansi(text: &str) -> std::result::Result<Vec<u8>, char> {
    text.chars()
        .map(|c| match c as u32 {
            0x20..=0x7E | 0xA0..=0xFF => Ok(c as u32 as u8),
            _ => match c {
                '€' => Ok(0x80),
                '…' => Ok(0x85),
                '‘' => Ok(0x91),
                '’' => Ok(0x92),
                '“' => Ok(0x93),
                '”' => Ok(0x94),
                '•' => Ok(0x95),
                '–' => Ok(0x96),
                '—' => Ok(0x97),
                _ => Err(c),
            },
        })
        .collect()
}

pub fn text_width(bytes: &[u8], size: f64) -> f64 {
    bytes
        .iter()
        .map(|&b| {
            if (32..=126).contains(&b) {
                HELVETICA[(b - 32) as usize] as f64
            } else {
                556.0
            }
        })
        .sum::<f64>()
        * size
        / 1000.0
}

pub(crate) fn pdf_string(bytes: &[u8]) -> String {
    let mut s = String::from("(");
    for &b in bytes {
        match b {
            b'(' | b')' | b'\\' => {
                s.push('\\');
                s.push(b as char);
            }
            0x20..=0x7E => s.push(b as char),
            _ => s.push_str(&format!("\\{b:03o}")),
        }
    }
    s.push(')');
    s
}

/// One thing to draw on a page, in display coordinates.
pub enum Mark {
    Text {
        bytes: Vec<u8>,
        size: f64,
        color: [u8; 4],
        x: f64,
        y: f64,
        angle: f64,
    },
    Image {
        name: String,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        angle: f64,
    },
}

/// Draws marks on top of pages.
#[derive(Default)]
pub struct Stamper {
    font: Option<ObjectId>,
    states: HashMap<u8, ObjectId>,
    images: Vec<(String, ObjectId)>,
}

impl Stamper {
    pub fn new() -> Stamper {
        Stamper::default()
    }

    /// Embed an image once; returns the resource name for `Mark::Image`.
    pub fn add_image(&mut self, doc: &mut Document, img: &PdfImage) -> String {
        let id = embed_image(doc, img);
        let name = format!("DeftIm{}", self.images.len() + 1);
        self.images.push((name.clone(), id));
        name
    }

    /// Draw `marks` on a page. `opacity` is 0-255.
    pub fn stamp(
        &mut self,
        doc: &mut Document,
        page: ObjectId,
        marks: &[Mark],
        opacity: u8,
    ) -> Result<()> {
        let font = match self.font {
            Some(f) => f,
            None => {
                let f = doc.add_object(dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding" });
                self.font = Some(f);
                f
            }
        };
        let gs = match self.states.get(&opacity) {
            Some(id) => *id,
            None => {
                let a = opacity as f32 / 255.0;
                let id =
                    doc.add_object(dictionary! { "Type" => "ExtGState", "ca" => a, "CA" => a });
                self.states.insert(opacity, id);
                id
            }
        };
        let gs_name = format!("DeftGs{opacity}");
        let m = display_matrix(doc, page);
        let mut ops = format!(
            "q {} {} {} {} {} {} cm /{gs_name} gs\n",
            m[0], m[1], m[2], m[3], m[4], m[5]
        );
        for mark in marks {
            match mark {
                Mark::Text {
                    bytes,
                    size,
                    color,
                    x,
                    y,
                    angle,
                } => {
                    let (s, c) = angle.to_radians().sin_cos();
                    let [r, g, b, _] = color.map(|v| v as f64 / 255.0);
                    ops.push_str(&format!(
                        "BT /DeftF1 {size:.2} Tf {r:.3} {g:.3} {b:.3} rg {c:.5} {s:.5} {:.5} {c:.5} {x:.2} {y:.2} Tm {} Tj ET\n",
                        -s,
                        pdf_string(bytes)
                    ));
                }
                Mark::Image {
                    name,
                    x,
                    y,
                    w,
                    h,
                    angle,
                } => {
                    let (s, c) = angle.to_radians().sin_cos();
                    ops.push_str(&format!(
                        "q {:.5} {:.5} {:.5} {:.5} {x:.2} {y:.2} cm /{name} Do Q\n",
                        w * c,
                        w * s,
                        -h * s,
                        h * c
                    ));
                }
            }
        }
        ops.push_str("Q\n");
        // Resources: copy into the page so shared dictionaries stay untouched.
        let mut res = match doc
            .get_dictionary(page)
            .ok()
            .and_then(|d| d.get(b"Resources").ok())
        {
            Some(o) => deref(doc, o).as_dict().cloned().unwrap_or_default(),
            None => Dictionary::new(),
        };
        let mut add = |category: &[u8], name: &str, id: ObjectId| {
            let mut sub = res
                .get(category)
                .ok()
                .map(|o| deref(doc, o))
                .and_then(|o| o.as_dict().ok())
                .cloned()
                .unwrap_or_default();
            sub.set(name.as_bytes().to_vec(), Object::Reference(id));
            res.set(category.to_vec(), Object::Dictionary(sub));
        };
        add(b"Font", "DeftF1", font);
        add(b"ExtGState", &gs_name, gs);
        for (name, id) in &self.images {
            if marks
                .iter()
                .any(|m| matches!(m, Mark::Image { name: n, .. } if n == name))
            {
                add(b"XObject", name, *id);
            }
        }
        // Wrap the existing content in q/Q so its state cannot leak into ours.
        let old: Vec<Object> = match doc
            .get_dictionary(page)
            .ok()
            .and_then(|d| d.get(b"Contents").ok())
        {
            Some(Object::Reference(id)) => match doc.get_object(*id) {
                Ok(Object::Array(a)) => a.clone(),
                _ => vec![Object::Reference(*id)],
            },
            Some(Object::Array(a)) => a.clone(),
            _ => vec![],
        };
        let pre = doc.add_object(Stream::new(Dictionary::new(), b"q\n".to_vec()));
        let mut post_stream = Stream::new(Dictionary::new(), format!("Q\n{ops}").into_bytes());
        let _ = post_stream.compress();
        let post = doc.add_object(post_stream);
        let mut contents = vec![Object::Reference(pre)];
        contents.extend(old);
        contents.push(Object::Reference(post));
        let dict = doc
            .get_dictionary_mut(page)
            .map_err(|e| err(ErrorCode::InputCorrupt, e.to_string()))?;
        dict.set("Contents", Object::Array(contents));
        dict.set("Resources", Object::Dictionary(res));
        Ok(())
    }
}

// ------------------------------------------------------------------ images

/// An image ready to embed.
pub enum PdfImage {
    /// JPEG bytes passed through unchanged (DCTDecode).
    Jpeg {
        bytes: Vec<u8>,
        width: u32,
        height: u32,
        components: u8,
    },
    /// 8-bit raw samples, Flate-compressed on embedding.
    Raw {
        samples: Vec<u8>,
        width: u32,
        height: u32,
        gray: bool,
        alpha: Option<Vec<u8>>,
    },
}

impl PdfImage {
    pub fn size(&self) -> (u32, u32) {
        match self {
            PdfImage::Jpeg { width, height, .. } | PdfImage::Raw { width, height, .. } => {
                (*width, *height)
            }
        }
    }

    /// Prepare image bytes for embedding. Upright JPEGs pass through
    /// untouched; everything else is decoded (orientation applied).
    pub fn from_bytes(
        bytes: &[u8],
        format: deft_core::format::Format,
        max_pixels: u64,
    ) -> Result<PdfImage> {
        use deft_core::format::Format;
        if format == Format::Jpeg
            && deft_tools_image::exif_orientation(bytes) == 1
            && let Some((width, height, components)) = deft_tools_image::jpeg_info(bytes)
            && (components == 1 || components == 3)
        {
            return Ok(PdfImage::Jpeg {
                bytes: bytes.to_vec(),
                width,
                height,
                components,
            });
        }
        let img = deft_tools_image::decode(bytes, format, max_pixels)?;
        if format == Format::Jpeg {
            // Rotated or CMYK JPEG: re-encode upright at high quality.
            let jpg = deft_tools_image::encode(
                &img,
                Format::Jpeg,
                deft_tools_image::Encode {
                    quality: 92,
                    ..Default::default()
                },
            )?;
            let (width, height, components) = deft_tools_image::jpeg_info(&jpg)
                .ok_or_else(|| err(ErrorCode::Internal, "jpeg re-encode"))?;
            return Ok(PdfImage::Jpeg {
                bytes: jpg,
                width,
                height,
                components,
            });
        }
        Ok(PdfImage::from_image(&img))
    }

    pub fn from_image(img: &image::DynamicImage) -> PdfImage {
        let (width, height) = (img.width(), img.height());
        let gray = !img.color().has_color();
        let alpha = if deft_tools_image::has_transparency(img) {
            Some(img.to_rgba8().pixels().map(|p| p[3]).collect())
        } else {
            None
        };
        let samples = if gray {
            img.to_luma8().into_raw()
        } else {
            img.to_rgb8().into_raw()
        };
        PdfImage::Raw {
            samples,
            width,
            height,
            gray,
            alpha,
        }
    }
}

fn flate(data: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut enc = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(data).expect("in-memory write");
    enc.finish().expect("in-memory write")
}

pub fn embed_image(doc: &mut Document, img: &PdfImage) -> ObjectId {
    match img {
        PdfImage::Jpeg {
            bytes,
            width,
            height,
            components,
        } => {
            let cs = if *components == 1 {
                "DeviceGray"
            } else {
                "DeviceRGB"
            };
            let dict = dictionary! {
                "Type" => "XObject", "Subtype" => "Image", "Width" => *width as i64, "Height" => *height as i64,
                "ColorSpace" => cs, "BitsPerComponent" => 8, "Filter" => "DCTDecode",
            };
            doc.add_object(Stream::new(dict, bytes.clone()).with_compression(false))
        }
        PdfImage::Raw {
            samples,
            width,
            height,
            gray,
            alpha,
        } => {
            let cs = if *gray { "DeviceGray" } else { "DeviceRGB" };
            let mut dict = dictionary! {
                "Type" => "XObject", "Subtype" => "Image", "Width" => *width as i64, "Height" => *height as i64,
                "ColorSpace" => cs, "BitsPerComponent" => 8, "Filter" => "FlateDecode",
            };
            if let Some(a) = alpha {
                let mask = dictionary! {
                    "Type" => "XObject", "Subtype" => "Image", "Width" => *width as i64, "Height" => *height as i64,
                    "ColorSpace" => "DeviceGray", "BitsPerComponent" => 8, "Filter" => "FlateDecode",
                };
                let mask_id = doc.add_object(Stream::new(mask, flate(a)).with_compression(false));
                dict.set("SMask", Object::Reference(mask_id));
            }
            doc.add_object(Stream::new(dict, flate(samples)).with_compression(false))
        }
    }
}

/// Page layout for images-to-PDF.
#[derive(Clone, Copy, Debug)]
pub struct PageSetup {
    /// None: each page takes the image's size (at 96 dpi).
    pub paper: Option<(f64, f64)>,
    /// None chooses per image.
    pub landscape: Option<bool>,
    pub margin: f64,
}

/// One page per image, each image centred and scaled to fit.
pub fn images_to_pdf(images: &[PdfImage], setup: PageSetup) -> Result<Document> {
    let mut doc = Document::with_version("1.7");
    let tree_id = doc.new_object_id();
    let mut kids = Vec::new();
    for img in images {
        let (iw, ih) = img.size();
        let (iw_pt, ih_pt) = (iw as f64 * 0.75, ih as f64 * 0.75);
        let (pw, ph) = match setup.paper {
            None => (iw_pt + 2.0 * setup.margin, ih_pt + 2.0 * setup.margin),
            Some((a, b)) => {
                if setup.landscape.unwrap_or(iw > ih) {
                    (a.max(b), a.min(b))
                } else {
                    (a.min(b), a.max(b))
                }
            }
        };
        let (bw, bh) = (
            (pw - 2.0 * setup.margin).max(1.0),
            (ph - 2.0 * setup.margin).max(1.0),
        );
        let scale = if setup.paper.is_none() {
            1.0
        } else {
            (bw / iw_pt).min(bh / ih_pt)
        };
        let (dw, dh) = (iw_pt * scale, ih_pt * scale);
        let (x, y) = ((pw - dw) / 2.0, (ph - dh) / 2.0);
        let image_id = embed_image(&mut doc, img);
        let content = format!("q {dw:.3} 0 0 {dh:.3} {x:.3} {y:.3} cm /Im1 Do Q\n");
        let content_id = doc.add_object(Stream::new(Dictionary::new(), content.into_bytes()));
        let page = dictionary! {
            "Type" => "Page", "Parent" => tree_id,
            "MediaBox" => vec![0.into(), 0.into(), Object::Real(pw as f32), Object::Real(ph as f32)],
            "Resources" => dictionary! { "XObject" => dictionary! { "Im1" => image_id } },
            "Contents" => content_id,
        };
        kids.push(Object::Reference(doc.add_object(page)));
    }
    let count = kids.len() as i64;
    doc.objects.insert(
        tree_id,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => count }),
    );
    let catalog = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => tree_id });
    doc.trailer.set("Root", catalog);
    Ok(doc)
}

// ------------------------------------------------------------------ compression

#[derive(Clone, Copy, Debug)]
pub struct ImagePolicy {
    /// Target resolution for the largest page side.
    pub dpi: f64,
    pub quality: u8,
    pub grayscale: bool,
}

/// Structural clean-up: compress streams, drop unused objects.
pub fn compress_structure(doc: &mut Document) {
    doc.delete_zero_length_streams();
    prune(doc);
    doc.compress();
}

/// Re-encode embedded 8-bit RGB/gray images as JPEG, downsampled to the
/// policy's resolution. An image is replaced only when the result is smaller.
/// Returns how many images were replaced.
pub fn recompress_images(doc: &mut Document, policy: ImagePolicy) -> usize {
    // The largest page side bounds the useful pixel count of any image.
    let max_side_pt = page_ids(doc)
        .iter()
        .map(|&p| {
            let b = page_box(doc, p);
            (b[2] - b[0]).max(b[3] - b[1])
        })
        .fold(0.0, f64::max);
    let cap = ((max_side_pt / 72.0) * policy.dpi).ceil().max(64.0) as u32;
    let masks: HashSet<ObjectId> = doc
        .objects
        .values()
        .filter_map(|o| o.as_stream().ok())
        .filter_map(|s| s.dict.get(b"SMask").and_then(Object::as_reference).ok())
        .collect();
    let candidates: Vec<(ObjectId, &Stream, usize)> = doc
        .objects
        .iter()
        .filter(|(id, _)| !masks.contains(id))
        .filter_map(|(id, o)| o.as_stream().ok().map(|s| (*id, s)))
        .filter(|(_, s)| s.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image"))
        .filter_map(|(id, s)| {
            let space = s.dict.get(b"ColorSpace").ok()?;
            Some((id, s, extract::decodable_channels(doc, space)?))
        })
        .collect();
    let workers = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let replaced: Vec<(ObjectId, Stream)> = std::thread::scope(|scope| {
        let handles: Vec<_> = candidates
            .chunks(candidates.len().div_ceil(workers).max(1))
            .map(|chunk| {
                scope.spawn(move || {
                    chunk
                        .iter()
                        .filter_map(|(id, s, channels)| {
                            recompress_one(s, *channels, policy, cap).map(|n| (*id, n))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_default())
            .collect()
    });
    let n = replaced.len();
    let mut shrunk = Vec::new();
    for (id, stream) in replaced {
        let mask = stream
            .dict
            .get(b"SMask")
            .and_then(Object::as_reference)
            .ok();
        let get = |k: &[u8]| stream.dict.get(k).and_then(Object::as_i64).unwrap_or(0) as u32;
        if let Some(mask) = mask {
            shrunk.push((mask, get(b"Width"), get(b"Height")));
        }
        doc.objects.insert(id, Object::Stream(stream));
    }
    shrink_masks(doc, &shrunk);
    n
}

/// Downsample the soft masks of recompressed images to their new size. The
/// mask stays a lossless image of its own.
fn shrink_masks(doc: &mut Document, images: &[(ObjectId, u32, u32)]) {
    let mut uses: HashMap<ObjectId, usize> = HashMap::new();
    for o in doc.objects.values() {
        if let Ok(s) = o.as_stream()
            && let Ok(id) = s.dict.get(b"SMask").and_then(Object::as_reference)
        {
            *uses.entry(id).or_default() += 1;
        }
    }
    for &(id, w, h) in images {
        // A mask shared by several images keeps its size.
        if uses.get(&id) != Some(&1) || w == 0 || h == 0 {
            continue;
        }
        let Some(stream) = doc.objects.get(&id).and_then(|o| o.as_stream().ok()) else {
            continue;
        };
        if let Some(smaller) = shrink_mask(stream, w, h) {
            doc.objects.insert(id, Object::Stream(smaller));
        }
    }
}

fn shrink_mask(s: &Stream, w: u32, h: u32) -> Option<Stream> {
    let d = &s.dict;
    let get_i = |k: &[u8]| d.get(k).ok().and_then(|o| o.as_i64().ok());
    let (mw, mh) = (get_i(b"Width")? as u32, get_i(b"Height")? as u32);
    if get_i(b"BitsPerComponent") != Some(8) || d.has(b"Decode") || (mw <= w && mh <= h) {
        return None;
    }
    let filters = s.filters().unwrap_or_default();
    if !(filters.is_empty() || (filters.len() == 1 && filters[0] == b"FlateDecode")) {
        return None;
    }
    let raw = if filters.is_empty() {
        s.content.clone()
    } else {
        s.decompressed_content().ok()?
    };
    let gray = image::GrayImage::from_raw(mw, mh, raw)?;
    let small =
        deft_tools_image::resize_exact(&image::DynamicImage::ImageLuma8(gray), w, h).ok()?;
    let mut dict = d.clone();
    dict.set("Width", w as i64);
    dict.set("Height", h as i64);
    dict.set("Filter", "FlateDecode");
    dict.remove(b"DecodeParms");
    dict.remove(b"Length");
    Some(Stream::new(dict, flate(small.to_luma8().as_raw())).with_compression(false))
}

/// A CMYK or YCCK JPEG as RGB, decoded the way the page renderer (hayro,
/// after pdf.js) decodes it, so the picture keeps its colours.
fn cmyk_jpeg(data: &[u8], w: u32, h: u32) -> Option<image::DynamicImage> {
    use zune_core::bytestream::ZCursor;
    use zune_core::colorspace::ColorSpace;
    use zune_core::options::DecoderOptions;
    let mut decoder = zune_jpeg::JpegDecoder::new(ZCursor::new(data));
    decoder.decode_headers().ok()?;
    let ycck = decoder.input_colorspace()? == ColorSpace::YCCK;
    let out = if ycck {
        ColorSpace::YCCK
    } else {
        ColorSpace::CMYK
    };
    decoder.set_options(DecoderOptions::default().jpeg_set_out_colorspace(out));
    let mut pixels = decoder.decode().ok()?;
    if pixels.len() != (w * h * 4) as usize {
        return None;
    }
    if ycck {
        // As pdf.js does: YCC to CMY, with K as stored.
        for c in pixels.chunks_mut(4) {
            let (y, cb, cr) = (c[0] as f32, c[1] as f32, c[2] as f32);
            c[0] = (434.456 - y - 1.402 * cr) as u8;
            c[1] = (119.541 - y + 0.344 * cb + 0.714 * cr) as u8;
            c[2] = (481.816 - y - 1.772 * cb) as u8;
        }
    }
    // Plain CMYK to RGB, as the renderer does without a colour profile.
    let rgb: Vec<u8> = pixels
        .chunks_exact(4)
        .flat_map(|p| {
            let k = 255 - p[3] as u32;
            [0, 1, 2].map(|i| ((255 - p[i] as u32) * k / 255) as u8)
        })
        .collect();
    Some(image::DynamicImage::ImageRgb8(image::RgbImage::from_raw(
        w, h, rgb,
    )?))
}

fn recompress_one(s: &Stream, comps: usize, policy: ImagePolicy, cap: u32) -> Option<Stream> {
    let d = &s.dict;
    let get_i = |k: &[u8]| d.get(k).ok().and_then(|o| o.as_i64().ok());
    // A soft mask is a separate image and is downsampled after; a colour-key
    // /Mask needs exact colours, which JPEG does not keep. A soft mask
    // overrides /Mask, so then /Mask does not count.
    let soft = d.has(b"SMask");
    if d.get(b"ImageMask")
        .and_then(Object::as_bool)
        .unwrap_or(false)
        || d.has(b"Decode")
        || (d.has(b"Mask") && !soft)
    {
        return None;
    }
    if get_i(b"BitsPerComponent") != Some(8) {
        return None;
    }
    let (w, h) = (get_i(b"Width")? as u32, get_i(b"Height")? as u32);
    let filters = s.filters().unwrap_or_default();
    let img = if filters.len() == 1 && filters[0] == b"DCTDecode" && comps == 4 {
        cmyk_jpeg(&s.content, w, h)?
    } else if filters.len() == 1 && filters[0] == b"DCTDecode" {
        let img = image::load_from_memory_with_format(&s.content, image::ImageFormat::Jpeg).ok()?;
        if img.width() != w || img.height() != h {
            return None;
        }
        img
    } else if filters.is_empty() || (filters.len() == 1 && filters[0] == b"FlateDecode") {
        let raw = if filters.is_empty() {
            s.content.clone()
        } else {
            s.decompressed_content().ok()?
        };
        if raw.len() != (w * h) as usize * comps {
            return None;
        }
        if comps == 4 {
            // Plain CMYK to RGB; print PDFs keep their photos in CMYK.
            let rgb: Vec<u8> = raw
                .chunks_exact(4)
                .flat_map(|p| {
                    let k = 255 - p[3] as u32;
                    [0, 1, 2].map(|i| ((255 - p[i] as u32) * k / 255) as u8)
                })
                .collect();
            image::DynamicImage::ImageRgb8(image::RgbImage::from_raw(w, h, rgb)?)
        } else if comps == 3 {
            image::DynamicImage::ImageRgb8(image::RgbImage::from_raw(w, h, raw)?)
        } else {
            image::DynamicImage::ImageLuma8(image::GrayImage::from_raw(w, h, raw)?)
        }
    } else {
        return None;
    };
    let img = if w.max(h) > cap {
        let f = cap as f64 / w.max(h) as f64;
        deft_tools_image::resize_exact(
            &img,
            ((w as f64 * f).round() as u32).max(1),
            ((h as f64 * f).round() as u32).max(1),
        )
        .ok()?
    } else {
        img
    };
    let img = if policy.grayscale {
        image::DynamicImage::ImageLuma8(img.to_luma8())
    } else {
        img
    };
    let opt = deft_tools_image::Encode {
        quality: policy.quality,
        ..Default::default()
    };
    let jpg = deft_tools_image::encode(&img, deft_core::format::Format::Jpeg, opt).ok()?;
    if jpg.len() >= s.content.len() {
        return None;
    }
    let mut dict = d.clone();
    if soft {
        dict.remove(b"Mask");
    }
    dict.set("Filter", "DCTDecode");
    dict.remove(b"DecodeParms");
    dict.set("Width", img.width() as i64);
    dict.set("Height", img.height() as i64);
    // An ICC profile stays valid while the channel count does.
    let channels = if img.color().has_color() { 3 } else { 1 };
    if channels != comps {
        let space = if channels == 3 {
            "DeviceRGB"
        } else {
            "DeviceGray"
        };
        dict.set("ColorSpace", space);
    }
    dict.remove(b"Length");
    Some(Stream::new(dict, jpg).with_compression(false))
}

// ------------------------------------------------------------------ rendering

/// A parsed document ready for rendering several pages.
pub struct Renderer {
    pdf: hayro::hayro_syntax::Pdf,
}

impl Renderer {
    pub fn new(bytes: &[u8]) -> Result<Renderer> {
        let pdf = hayro::hayro_syntax::Pdf::new(bytes.to_vec()).map_err(|e| match e {
            hayro::hayro_syntax::LoadPdfError::Decryption(_) => err(
                ErrorCode::InputEncrypted,
                "cannot render a password-protected PDF",
            ),
            _ => err(
                ErrorCode::InputCorrupt,
                "could not parse the PDF for rendering",
            ),
        })?;
        Ok(Renderer { pdf })
    }

    /// A renderer for a loaded document, which is already decrypted. What is
    /// left of /Encrypt (attachments only) stays out, as the renderer would
    /// ask for the password.
    pub fn of(doc: &Document) -> Result<Renderer> {
        let mut copy = doc.clone();
        copy.encryption_state = None;
        copy.trailer.remove(b"Encrypt");
        fonts::add_text_maps(&mut copy);
        let mut out = Vec::new();
        copy.save_to(&mut out)
            .map_err(|e| err(ErrorCode::Internal, format!("reading PDF: {e}")))?;
        Renderer::new(&out)
    }

    pub fn page_count(&self) -> usize {
        self.pdf.pages().len()
    }

    /// Render one page (zero-based) at `dpi` to an RGB image on white.
    pub fn render(&self, index: usize, dpi: f64, max_pixels: u64) -> Result<image::RgbImage> {
        let pages = self.pdf.pages();
        let page = pages.get(index).ok_or_else(|| {
            err(
                ErrorCode::BadValue,
                format!("page {} does not exist", index + 1),
            )
        })?;
        let scale = (dpi / 72.0) as f32;
        let (w, h) = page.render_dimensions();
        let (pw, ph) = (w * scale, h * scale);
        if pw as u64 * ph as u64 > max_pixels || pw > 65000.0 || ph > 65000.0 {
            return Err(err(
                ErrorCode::LimitExceeded,
                format!("page {} at {dpi} dpi is too large", index + 1),
            )
            .hint("use a lower --dpi"));
        }
        let cache = hayro::RenderCache::new();
        // A hair above the exact scale, so float error cannot lose a pixel row.
        let (sx, sy) = ((pw.round() + 0.01) / w, (ph.round() + 0.01) / h);
        let settings = hayro::PixmapSettings {
            x_scale: sx,
            y_scale: sy,
            bg_color: hayro::vello_cpu::color::palette::css::WHITE,
        };
        let pixmap = hayro::render(
            page,
            &cache,
            &Default::default(),
            &hayro::RenderSettings::default(),
            &settings,
        );
        let (pw, ph) = (pixmap.width() as u32, pixmap.height() as u32);
        let mut rgb = Vec::with_capacity((pw * ph * 3) as usize);
        for px in pixmap.data_as_u8_slice().as_chunks::<4>().0 {
            rgb.extend_from_slice(&px[..3]);
        }
        image::RgbImage::from_raw(pw, ph, rgb)
            .ok_or_else(|| err(ErrorCode::Internal, "render buffer"))
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
