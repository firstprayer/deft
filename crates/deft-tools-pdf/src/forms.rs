//! AcroForm fields: listing, filling and flattening.

use crate::meta::text_object;
use crate::pages::{annot_rect, draw_forms};
use crate::{deref, num, page_ids, pdf_string, rect_of, text_string, text_width, win_ansi};
use deft_core::error::{ErrorCode, Result, err};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};

/// One logical form field.
#[derive(Clone, Debug)]
pub struct Field {
    pub name: String,
    pub kind: &'static str,
    pub value: Option<String>,
    /// Choices, or the on-state of a checkbox or radio group.
    pub options: Vec<String>,
    pub required: bool,
    pub read_only: bool,
    id: ObjectId,
    widgets: Vec<ObjectId>,
    flags: i64,
    max_len: Option<i64>,
    default_appearance: Option<String>,
}

impl Field {
    pub fn to_json(&self) -> Value {
        let mut v = json!({"name": self.name, "type": self.kind});
        if let Some(x) = &self.value {
            v["value"] = json!(x);
        }
        if !self.options.is_empty() {
            v["options"] = json!(self.options);
        }
        if self.required {
            v["required"] = json!(true);
        }
        if self.read_only {
            v["read_only"] = json!(true);
        }
        if let Some(n) = self.max_len.filter(|n| *n > 0 && self.kind == "text") {
            v["max_length"] = json!(n);
        }
        v
    }
}

fn inherited_field<'a>(doc: &'a Document, id: ObjectId, key: &[u8]) -> Option<&'a Object> {
    let mut at = id;
    for _ in 0..32 {
        let d = doc.get_dictionary(at).ok()?;
        if let Ok(v) = d.get(key) {
            return Some(v);
        }
        at = d.get(b"Parent").and_then(Object::as_reference).ok()?;
    }
    None
}

fn kids(doc: &Document, id: ObjectId) -> Vec<ObjectId> {
    doc.get_dictionary(id)
        .ok()
        .and_then(|d| d.get(b"Kids").ok())
        .map(|o| deref(doc, o))
        .and_then(|o| o.as_array().ok())
        .map(|a| a.iter().filter_map(|o| o.as_reference().ok()).collect())
        .unwrap_or_default()
}

/// A checkbox widget's on-state: the appearance name that is not Off.
fn on_states(doc: &Document, widget: ObjectId) -> Vec<String> {
    let normal = doc
        .get_dictionary(widget)
        .ok()
        .and_then(|d| d.get(b"AP").ok())
        .map(|o| deref(doc, o))
        .and_then(|o| o.as_dict().ok())
        .and_then(|ap| ap.get(b"N").ok())
        .map(|o| deref(doc, o))
        .and_then(|o| o.as_dict().ok());
    normal
        .map(|n| {
            n.iter()
                .map(|(k, _)| String::from_utf8_lossy(k).to_string())
                .filter(|k| k != "Off")
                .collect()
        })
        .unwrap_or_default()
}

fn name_or_text(doc: &Document, o: &Object) -> Option<String> {
    match deref(doc, o) {
        Object::Name(n) => Some(String::from_utf8_lossy(n).to_string()),
        other => text_string(doc, other),
    }
}

fn collect(
    doc: &Document,
    id: ObjectId,
    prefix: &str,
    out: &mut Vec<Field>,
    seen: &mut HashSet<ObjectId>,
) {
    if !seen.insert(id) {
        return;
    }
    let Ok(dict) = doc.get_dictionary(id) else {
        return;
    };
    let part = dict
        .get(b"T")
        .ok()
        .and_then(|o| text_string(doc, o))
        .unwrap_or_default();
    let name = match (prefix.is_empty(), part.is_empty()) {
        (true, _) => part,
        (false, true) => prefix.to_string(),
        (false, false) => format!("{prefix}.{part}"),
    };
    let children = kids(doc, id);
    // Kids with their own /T are sub-fields; kids without are this field's widgets.
    let has_subfields = children
        .iter()
        .any(|k| doc.get_dictionary(*k).map(|d| d.has(b"T")).unwrap_or(false));
    if has_subfields {
        for k in children {
            collect(doc, k, &name, out, seen);
        }
        return;
    }
    let widgets = if children.is_empty() {
        vec![id]
    } else {
        children
    };
    let ft = inherited_field(doc, id, b"FT").and_then(|o| deref(doc, o).as_name().ok());
    let flags = inherited_field(doc, id, b"Ff")
        .and_then(|o| num(deref(doc, o)))
        .unwrap_or(0.0) as i64;
    let (kind, mut options) = match ft {
        Some(b"Tx") => ("text", vec![]),
        Some(b"Btn") if flags & (1 << 16) != 0 => ("button", vec![]),
        Some(b"Btn") => {
            let states: Vec<String> = widgets.iter().flat_map(|w| on_states(doc, *w)).collect();
            (
                if flags & (1 << 15) != 0 {
                    "radio"
                } else {
                    "checkbox"
                },
                states,
            )
        }
        Some(b"Ch") => ("choice", vec![]),
        Some(b"Sig") => ("signature", vec![]),
        _ => ("unknown", vec![]),
    };
    if kind == "choice"
        && let Some(list) = inherited_field(doc, id, b"Opt")
            .map(|o| deref(doc, o))
            .and_then(|o| o.as_array().ok())
    {
        for item in list {
            // An option is a string, or an array of [export, display].
            let text = match deref(doc, item) {
                Object::Array(a) => a.first().and_then(|o| text_string(doc, o)),
                other => text_string(doc, other),
            };
            options.extend(text);
        }
    }
    let default_appearance = inherited_field(doc, id, b"DA")
        .and_then(|o| text_string(doc, o))
        .or_else(|| {
            doc.catalog()
                .ok()?
                .get(b"AcroForm")
                .ok()
                .map(|o| deref(doc, o))?
                .as_dict()
                .ok()?
                .get(b"DA")
                .ok()
                .and_then(|o| text_string(doc, o))
        });
    out.push(Field {
        name,
        kind,
        value: inherited_field(doc, id, b"V")
            .and_then(|o| name_or_text(doc, o))
            .filter(|v| !v.is_empty() && v != "Off"),
        options,
        required: flags & 2 != 0,
        read_only: flags & 1 != 0,
        id,
        widgets,
        flags,
        max_len: inherited_field(doc, id, b"MaxLen")
            .and_then(|o| num(deref(doc, o)))
            .map(|n| n as i64),
        default_appearance,
    });
}

pub(crate) fn acro_form(doc: &Document) -> Option<&Dictionary> {
    let catalog = doc.catalog().ok()?;
    deref(doc, catalog.get(b"AcroForm").ok()?).as_dict().ok()
}

/// Whether the form is a dynamic XFA form: its pages and fields live in XFA
/// data that only some viewers render, and the PDF page is a placeholder.
pub fn is_dynamic_xfa(doc: &Document) -> bool {
    let needs_rendering = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"NeedsRendering").ok())
        .and_then(|o| deref(doc, o).as_bool().ok())
        .unwrap_or(false);
    needs_rendering && acro_form(doc).is_some_and(|f| f.has(b"XFA"))
}

/// The warning for a dynamic XFA form.
pub const XFA_WARNING: &str = "this is a dynamic XFA form: its real fields and values are in \
     XFA data, which deft cannot read or fill; the PDF page is a placeholder";

/// Every field of the document's form, in document order.
pub fn fields(doc: &Document) -> Vec<Field> {
    let Some(form) = acro_form(doc) else {
        return vec![];
    };
    let roots: Vec<ObjectId> = form
        .get(b"Fields")
        .ok()
        .map(|o| deref(doc, o))
        .and_then(|o| o.as_array().ok())
        .map(|a| a.iter().filter_map(|o| o.as_reference().ok()).collect())
        .unwrap_or_default();
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for r in roots {
        collect(doc, r, "", &mut out, &mut seen);
    }
    out
}

/// The form dictionary for changing, wherever the catalog keeps it.
fn acro_form_mut(doc: &mut Document) -> Option<&mut Dictionary> {
    let id = doc
        .catalog()
        .ok()?
        .get(b"AcroForm")
        .ok()?
        .as_reference()
        .ok();
    match id {
        Some(id) => doc.get_dictionary_mut(id).ok(),
        None => doc
            .catalog_mut()
            .ok()?
            .get_mut(b"AcroForm")
            .ok()?
            .as_dict_mut()
            .ok(),
    }
}

fn root_fields(doc: &Document, form: &Dictionary) -> Vec<Object> {
    form.get(b"Fields")
        .ok()
        .map(|o| deref(doc, o))
        .and_then(|o| o.as_array().ok())
        .cloned()
        .unwrap_or_default()
}

/// Whether this field still has a place on the page, trimming widgets that do not.
fn is_placed(doc: &mut Document, id: ObjectId, placed: &HashSet<ObjectId>) -> bool {
    let children = kids(doc, id);
    if children.is_empty() {
        let widget = doc
            .get_dictionary(id)
            .is_ok_and(|d| d.has(b"Rect") || d.has(b"Subtype"));
        return !widget || placed.contains(&id);
    }
    let left: Vec<ObjectId> = children
        .iter()
        .copied()
        .filter(|k| is_placed(doc, *k, placed))
        .collect();
    if left.len() != children.len()
        && let Ok(d) = doc.get_dictionary_mut(id)
    {
        d.set(
            "Kids",
            left.iter()
                .map(|k| Object::Reference(*k))
                .collect::<Vec<_>>(),
        );
    }
    !left.is_empty()
}

/// Drop every field with no widget on these pages, and so its value. An XFA
/// packet holds its own copy of the values, so it goes too.
pub(crate) fn keep_fields_on(doc: &mut Document, pages: &[ObjectId]) {
    let Some(form) = acro_form(doc) else {
        return;
    };
    let roots = root_fields(doc, form);
    let placed: HashSet<ObjectId> = pages
        .iter()
        .filter_map(|p| doc.get_dictionary(*p).ok()?.get(b"Annots").ok())
        .filter_map(|o| deref(doc, o).as_array().ok())
        .flatten()
        .filter_map(|o| o.as_reference().ok())
        .collect();
    let left: Vec<Object> = roots
        .iter()
        .filter(|o| o.as_reference().is_ok_and(|id| is_placed(doc, id, &placed)))
        .cloned()
        .collect();
    let empty = left.is_empty();
    let changed = left.len() != roots.len();
    if let Some(form) = acro_form_mut(doc) {
        form.set("Fields", left);
        if changed {
            form.remove(b"XFA");
        }
    }
    if empty && let Ok(catalog) = doc.catalog_mut() {
        catalog.remove(b"AcroForm");
    }
}

/// Remove the XFA packet of a form that also has plain fields. Returns false when
/// the XFA packet is the whole form and has to stay.
pub(crate) fn drop_xfa(doc: &mut Document) -> bool {
    let Some(form) = acro_form(doc) else {
        return true;
    };
    if !form.has(b"XFA") {
        return true;
    }
    if root_fields(doc, form).is_empty() {
        return false;
    }
    remove_xfa(doc);
    true
}

/// Remove the XFA packet, which holds its own copy of the field values.
pub(crate) fn remove_xfa(doc: &mut Document) {
    if let Some(form) = acro_form_mut(doc) {
        form.remove(b"XFA");
    }
}

fn field_name(doc: &Document, field: &Object) -> Option<String> {
    let dict = deref(doc, field).as_dict().ok()?;
    text_string(doc, dict.get(b"T").ok()?)
}

/// A name for a field that is not in `taken`.
fn free_name(name: &str, taken: &HashSet<String>) -> String {
    (2..)
        .map(|n| format!("{name}-{n}"))
        .find(|candidate| !taken.contains(candidate))
        .unwrap_or_else(|| name.to_string())
}

/// Add the form of a merged document to this one. Its objects must already be
/// part of `doc`. Fields sharing a name share a value, so a clash gets a new name.
pub(crate) fn append_form(doc: &mut Document, other: &Dictionary) {
    let added = root_fields(doc, other);
    if added.is_empty() {
        return;
    }
    let Some(form) = acro_form(doc) else {
        let mut copy = other.clone();
        copy.remove(b"XFA");
        if let Ok(catalog) = doc.catalog_mut() {
            catalog.set("AcroForm", Object::Dictionary(copy));
        }
        return;
    };
    let mut fields = root_fields(doc, form);
    let mut taken: HashSet<String> = fields.iter().filter_map(|f| field_name(doc, f)).collect();
    for field in &added {
        let Some(name) = field_name(doc, field) else {
            continue;
        };
        let fresh = if taken.contains(&name) {
            free_name(&name, &taken)
        } else {
            name
        };
        if let Ok(id) = field.as_reference()
            && let Ok(d) = doc.get_dictionary_mut(id)
        {
            d.set("T", text_object(&fresh));
        }
        taken.insert(fresh);
    }
    fields.extend(added);
    let resources = merged_resources(doc, other);
    let appearances = other
        .get(b"NeedAppearances")
        .and_then(Object::as_bool)
        .unwrap_or(false);
    if let Some(form) = acro_form_mut(doc) {
        form.set("Fields", fields);
        // The packet describes the first form only, and viewers prefer it.
        form.remove(b"XFA");
        if let Some(resources) = resources {
            form.set("DR", resources);
        }
        if appearances {
            form.set("NeedAppearances", true);
        }
    }
}

/// The default resources of both forms. Field appearances name fonts in them.
fn merged_resources(doc: &Document, other: &Dictionary) -> Option<Dictionary> {
    let dict = |form: &Dictionary, key: &[u8]| {
        form.get(key)
            .ok()
            .map(|o| deref(doc, o))
            .and_then(|o| o.as_dict().ok())
            .cloned()
    };
    let theirs = dict(other, b"DR")?;
    let mut ours = acro_form(doc)
        .and_then(|f| dict(f, b"DR"))
        .unwrap_or_default();
    for (category, value) in theirs.iter() {
        let Some(names) = deref(doc, value).as_dict().ok() else {
            continue;
        };
        let mut joined = dict(&ours, category).unwrap_or_default();
        for (name, item) in names.iter() {
            if !joined.has(name) {
                joined.set(name.clone(), item.clone());
            }
        }
        ours.set(category.clone(), Object::Dictionary(joined));
    }
    Some(ours)
}

/// Size from a default appearance string such as `/Helv 10 Tf 0 g`.
fn font_size(appearance: Option<&str>) -> f64 {
    let words: Vec<&str> = appearance.unwrap_or("").split_whitespace().collect();
    words
        .iter()
        .position(|w| *w == "Tf")
        .and_then(|i| i.checked_sub(1))
        .and_then(|i| words[i].parse().ok())
        .unwrap_or(0.0)
}

/// The colour operator of a default appearance, as written.
fn color_ops(appearance: Option<&str>) -> String {
    let words: Vec<&str> = appearance.unwrap_or("").split_whitespace().collect();
    for (i, w) in words.iter().enumerate() {
        let n = match *w {
            "g" => 1,
            "rg" => 3,
            "k" => 4,
            _ => continue,
        };
        if i >= n {
            return words[i - n..=i].join(" ");
        }
    }
    "0 g".into()
}

fn wrap_lines(text: &[u8], size: f64, width: f64) -> Vec<Vec<u8>> {
    let mut lines = Vec::new();
    for para in text.split(|b| *b == b'\n') {
        let mut line: Vec<u8> = Vec::new();
        for word in para.split(|b| *b == b' ') {
            let mut candidate = line.clone();
            if !candidate.is_empty() {
                candidate.push(b' ');
            }
            candidate.extend_from_slice(word);
            if text_width(&candidate, size) > width && !line.is_empty() {
                lines.push(std::mem::take(&mut line));
                line = word.to_vec();
            } else {
                line = candidate;
            }
        }
        lines.push(line);
    }
    lines
}

/// Build the appearance stream of a text or choice widget.
fn text_appearance(field: &Field, text: &str, rect: [f64; 4], font: ObjectId) -> Option<Stream> {
    let bytes = win_ansi(text).ok()?;
    let (w, h) = (rect[2] - rect[0], rect[3] - rect[1]);
    let da = field.default_appearance.as_deref();
    let multiline = field.kind == "text" && field.flags & (1 << 12) != 0;
    let comb = field.kind == "text" && field.flags & (1 << 24) != 0;
    let pad = 2.0;
    let mut size = font_size(da);
    if size <= 0.0 {
        size = if multiline {
            10.0
        } else {
            (h * 0.7).clamp(4.0, 12.0)
        };
        if !multiline {
            let room = w - 2.0 * pad;
            let full = text_width(&bytes, size);
            if full > room && full > 0.0 {
                size = (size * room / full).max(4.0);
            }
        }
    }
    let color = color_ops(da);
    let mut ops = format!("/Tx BMC q BT /DeftHelv {size:.2} Tf {color}\n");
    if comb && let Some(cells) = field.max_len.filter(|n| *n > 0) {
        let cell = w / cells as f64;
        let y = (h - size) / 2.0 + size * 0.22;
        for (i, b) in bytes.iter().enumerate().take(cells as usize) {
            let glyph = [*b];
            let x = cell * i as f64 + (cell - text_width(&glyph, size)) / 2.0;
            ops.push_str(&format!(
                "1 0 0 1 {x:.2} {y:.2} Tm {} Tj\n",
                pdf_string(&glyph)
            ));
        }
    } else if multiline {
        let leading = size * 1.15;
        let mut y = h - pad - size;
        for line in wrap_lines(&bytes, size, w - 2.0 * pad) {
            ops.push_str(&format!(
                "1 0 0 1 {pad} {y:.2} Tm {} Tj\n",
                pdf_string(&line)
            ));
            y -= leading;
        }
    } else {
        let y = (h - size) / 2.0 + size * 0.22;
        ops.push_str(&format!(
            "1 0 0 1 {pad} {y:.2} Tm {} Tj\n",
            pdf_string(&bytes)
        ));
    }
    ops.push_str("ET Q EMC\n");
    Some(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), Object::Real(w as f32), Object::Real(h as f32)],
            "Resources" => dictionary! { "Font" => dictionary! { "DeftHelv" => font } },
        },
        ops.into_bytes(),
    ))
}

fn is_truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::Null => false,
        Value::Number(n) => n.as_f64() != Some(0.0),
        Value::String(s) => !matches!(
            s.to_ascii_lowercase().as_str(),
            "" | "off" | "no" | "false" | "0"
        ),
        _ => true,
    }
}

fn scalar(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

/// A name without its [n] indexes: "f1_01" for "f1_01[0]".
fn without_indexes(name: &str) -> String {
    let mut out = String::with_capacity(name.len());
    let mut rest = name;
    while let Some(open) = rest.find('[') {
        let close = rest[open..].find(']').map(|c| open + c);
        match close {
            Some(close) if rest[open + 1..close].chars().all(|c| c.is_ascii_digit()) => {
                out.push_str(&rest[..open]);
                rest = &rest[close + 1..];
            }
            _ => {
                out.push_str(&rest[..=open]);
                rest = &rest[open + 1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// The field a name means: its full name, the last part of it when that is
/// unique, or either without the [0] indexes of XFA-made forms (IRS forms
/// name every field f1_01[0]) when that is unique.
fn find_field<'a>(all: &'a [Field], name: &str) -> Result<&'a Field> {
    if let Some(f) = all.iter().find(|f| f.name == name) {
        return Ok(f);
    }
    let last = |f: &Field| f.name.rsplit('.').next().unwrap_or("").to_string();
    let mut matches: Vec<&Field> = all.iter().filter(|f| last(f) == name).collect();
    if matches.is_empty() && !name.contains('[') {
        let plain = without_indexes(name);
        matches = all
            .iter()
            .filter(|f| without_indexes(&f.name) == plain || without_indexes(&last(f)) == plain)
            .collect();
    }
    match matches.as_slice() {
        [f] => Ok(f),
        [] => Err(err(ErrorCode::BadValue, format!("no field {name:?}"))
            .hint("list the names: deft pdf fields <file>")),
        many => Err(err(
            ErrorCode::BadValue,
            format!(
                "{name:?} matches {} fields ({}, ...); use the full name",
                many.len(),
                many[0].name
            ),
        )),
    }
}

/// Select options of a multi-select list box.
fn set_list(
    doc: &mut Document,
    f: &Field,
    value: &Value,
    font: ObjectId,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let picked: Vec<String> = value.as_array().into_iter().flatten().map(scalar).collect();
    for p in &picked {
        if !f.options.contains(p) {
            warnings.push(format!("{}: {p:?} is not one of its options", f.name));
        }
    }
    let indexes: Vec<Object> = f
        .options
        .iter()
        .enumerate()
        .filter(|(_, o)| picked.contains(o))
        .map(|(i, _)| Object::Integer(i as i64))
        .collect();
    let v = Object::Array(picked.iter().map(|p| text_object(p)).collect());
    set_on(doc, f.id, b"V", v)?;
    set_on(doc, f.id, b"I", Object::Array(indexes))?;
    for w in f.widgets.clone() {
        let rect = doc.get_dictionary(w).ok().and_then(|d| annot_rect(doc, d));
        if let Some(s) = rect.and_then(|r| list_appearance(f, &picked, r, font)) {
            let id = doc.add_object(s);
            set_on(doc, w, b"AP", Object::Dictionary(dictionary! { "N" => id }))?;
        }
    }
    Ok(())
}

/// A list box drawn with its options, the selected ones highlighted.
fn list_appearance(f: &Field, picked: &[String], rect: [f64; 4], font: ObjectId) -> Option<Stream> {
    let (w, h) = (rect[2] - rect[0], rect[3] - rect[1]);
    let da = f.default_appearance.as_deref();
    let size = match font_size(da) {
        s if s > 0.0 => s,
        _ => 10.0,
    };
    let leading = size * 1.15;
    let color = color_ops(da);
    let mut ops = String::from("/Tx BMC q\n");
    let mut top = h - 1.0;
    for option in &f.options {
        if top - leading < 0.0 {
            break;
        }
        let bytes = win_ansi(option).ok()?;
        let y = top - leading;
        if picked.contains(option) {
            ops.push_str(&format!(
                "0.6 0.75 0.9 rg 0 {y:.2} {w:.2} {leading:.2} re f\n"
            ));
        }
        ops.push_str(&format!(
            "BT /DeftHelv {size:.2} Tf {color} 1 0 0 1 2 {:.2} Tm {} Tj ET\n",
            y + size * 0.25,
            pdf_string(&bytes)
        ));
        top = y;
    }
    ops.push_str("Q EMC\n");
    Some(Stream::new(
        dictionary! {
            "Type" => "XObject",
            "Subtype" => "Form",
            "BBox" => vec![0.into(), 0.into(), Object::Real(w as f32), Object::Real(h as f32)],
            "Resources" => dictionary! { "Font" => dictionary! { "DeftHelv" => font } },
        },
        ops.into_bytes(),
    ))
}

/// A text field with /MaxLen holds and shows at most that many characters (a
/// comb field one per box), so a longer value would be cut on the page while
/// the form data kept all of it.
fn check_length(f: &Field, text: &str) -> Result<()> {
    let Some(max) = f.max_len.filter(|n| *n > 0 && f.kind == "text") else {
        return Ok(());
    };
    let n = text.chars().count();
    if n as i64 <= max {
        return Ok(());
    }
    let hint = if f.flags & (1 << 24) != 0 {
        "it has one box per character; a number spread over several fields, \
         such as an SSN, goes in parts into each of them"
    } else {
        "shorten the value; deft pdf fields shows each max_length"
    };
    Err(err(
        ErrorCode::BadValue,
        format!("{}: at most {max} characters, got {n}", f.name),
    )
    .hint(hint))
}

fn set_button(doc: &mut Document, f: &Field, value: &Value) -> Result<()> {
    // A radio group takes an export name; a checkbox takes true or false.
    let wanted = match value {
        Value::String(s) if f.options.iter().any(|o| o == s) => Some(s.clone()),
        v if f.kind == "checkbox" && is_truthy(v) => f.options.first().cloned(),
        v if f.kind == "radio" && is_truthy(v) => {
            return Err(err(
                ErrorCode::BadValue,
                format!("{}: pick one of {}", f.name, f.options.join(", ")),
            ));
        }
        _ => None,
    };
    let state = wanted.clone().unwrap_or_else(|| "Off".to_string());
    set_on(doc, f.id, b"V", Object::Name(state.clone().into_bytes()))?;
    for w in &f.widgets {
        let on = on_states(doc, *w);
        let this = if on.contains(&state) {
            state.clone()
        } else {
            "Off".to_string()
        };
        set_on(doc, *w, b"AS", Object::Name(this.into_bytes()))?;
    }
    Ok(())
}

fn set_on(doc: &mut Document, id: ObjectId, key: &[u8], value: Object) -> Result<()> {
    doc.get_dictionary_mut(id)
        .map_err(|e| err(ErrorCode::InputCorrupt, e.to_string()))?
        .set(key.to_vec(), value);
    Ok(())
}

/// Fill fields from name -> value pairs. Returns warnings. Unknown names are an
/// error, so a typo cannot pass as success.
pub fn fill(doc: &mut Document, values: &BTreeMap<String, Value>) -> Result<Vec<String>> {
    if is_dynamic_xfa(doc) {
        return Err(err(ErrorCode::InputUnsupported, XFA_WARNING));
    }
    let all = fields(doc);
    if all.is_empty() {
        return Err(err(
            ErrorCode::InputUnsupported,
            "the PDF has no form fields",
        ));
    }
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica", "Encoding" => "WinAnsiEncoding"
    });
    let mut warnings = Vec::new();
    let mut needs_viewer = false;
    for (name, value) in values {
        let f = find_field(&all, name)?;
        match f.kind {
            "choice" if value.is_array() && f.flags & (1 << 21) != 0 => {
                set_list(doc, f, value, font, &mut warnings)?;
            }
            _ if value.is_array() || value.is_object() => {
                let got = if value.is_array() {
                    "a list"
                } else {
                    "an object"
                };
                let want = if matches!(f.kind, "checkbox" | "radio") {
                    "true, false or an option"
                } else {
                    "text"
                };
                return Err(err(
                    ErrorCode::BadValue,
                    format!("{}: expected {want}, got {got}", f.name),
                ));
            }
            "checkbox" | "radio" => set_button(doc, f, value)?,
            "text" | "choice" => {
                let text = scalar(value);
                check_length(f, &text)?;
                if f.kind == "choice" && !f.options.is_empty() && !f.options.contains(&text) {
                    warnings.push(format!("{}: {text:?} is not one of its options", f.name));
                }
                set_on(doc, f.id, b"V", crate::meta::text_object(&text))?;
                for w in f.widgets.clone() {
                    let rect = doc.get_dictionary(w).ok().and_then(|d| annot_rect(doc, d));
                    let stream = rect.and_then(|r| text_appearance(f, &text, r, font));
                    match stream {
                        Some(s) => {
                            let id = doc.add_object(s);
                            set_on(doc, w, b"AP", Object::Dictionary(dictionary! { "N" => id }))?;
                        }
                        None => {
                            needs_viewer = true;
                            let d = doc
                                .get_dictionary_mut(w)
                                .map_err(|e| err(ErrorCode::InputCorrupt, e.to_string()))?;
                            d.remove(b"AP");
                        }
                    }
                }
            }
            other => warnings.push(format!("{}: cannot fill a {other} field", f.name)),
        }
    }
    // Forms with XFA ignore AcroForm values in some viewers.
    if let Ok(id) = doc
        .catalog()
        .and_then(|c| c.get(b"AcroForm"))
        .and_then(Object::as_reference)
    {
        let d = doc
            .get_dictionary_mut(id)
            .map_err(|e| err(ErrorCode::InputCorrupt, e.to_string()))?;
        d.remove(b"XFA");
        d.set("NeedAppearances", needs_viewer);
    } else if let Ok(catalog) = doc.catalog_mut()
        && let Ok(Object::Dictionary(d)) = catalog.get_mut(b"AcroForm")
    {
        d.remove(b"XFA");
        d.set("NeedAppearances", needs_viewer);
    }
    if needs_viewer {
        warnings.push(
            "some text is outside Latin-1: viewers will draw it, but flattening will not show it"
                .to_string(),
        );
    }
    Ok(warnings)
}

fn appearance(doc: &Document, annot: &Dictionary) -> Option<ObjectId> {
    let ap = deref(doc, annot.get(b"AP").ok()?).as_dict().ok()?;
    let n = ap.get(b"N").ok()?;
    if let Ok(id) = n.as_reference()
        && doc.get_object(id).is_ok_and(|o| o.as_stream().is_ok())
    {
        return Some(id);
    }
    let states = deref(doc, n).as_dict().ok()?;
    let state = annot.get(b"AS").ok()?.as_name().ok()?;
    states.get(state).ok()?.as_reference().ok()
}

fn matrix_of(doc: &Document, form: &Dictionary) -> [f64; 6] {
    let v: Option<Vec<f64>> = form
        .get(b"Matrix")
        .ok()
        .map(|o| deref(doc, o))
        .and_then(|o| o.as_array().ok())
        .and_then(|a| a.iter().map(|x| num(deref(doc, x))).collect());
    match v.as_deref() {
        Some([a, b, c, d, e, f]) => [*a, *b, *c, *d, *e, *f],
        _ => [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
    }
}

/// The matrix that places an appearance in an annotation's rectangle
/// (PDF 32000-1, 12.5.5, algorithm 8.1).
fn placement(bbox: [f64; 4], matrix: [f64; 6], rect: [f64; 4]) -> [f64; 6] {
    let pt = |x: f64, y: f64| {
        (
            matrix[0] * x + matrix[2] * y + matrix[4],
            matrix[1] * x + matrix[3] * y + matrix[5],
        )
    };
    let corners = [
        pt(bbox[0], bbox[1]),
        pt(bbox[2], bbox[1]),
        pt(bbox[0], bbox[3]),
        pt(bbox[2], bbox[3]),
    ];
    let min_x = corners.iter().map(|c| c.0).fold(f64::INFINITY, f64::min);
    let max_x = corners
        .iter()
        .map(|c| c.0)
        .fold(f64::NEG_INFINITY, f64::max);
    let min_y = corners.iter().map(|c| c.1).fold(f64::INFINITY, f64::min);
    let max_y = corners
        .iter()
        .map(|c| c.1)
        .fold(f64::NEG_INFINITY, f64::max);
    let sx = if max_x > min_x {
        (rect[2] - rect[0]) / (max_x - min_x)
    } else {
        1.0
    };
    let sy = if max_y > min_y {
        (rect[3] - rect[1]) / (max_y - min_y)
    } else {
        1.0
    };
    [sx, 0.0, 0.0, sy, rect[0] - min_x * sx, rect[1] - min_y * sy]
}

/// Paint form fields and annotations into the page and remove them, so nothing
/// can be edited. Links stay. Returns how many were flattened.
pub fn flatten(doc: &mut Document) -> Result<usize> {
    let mut total = 0;
    for page in page_ids(doc) {
        let annots: Vec<Object> = doc
            .get_dictionary(page)
            .ok()
            .and_then(|d| d.get(b"Annots").ok())
            .map(|o| deref(doc, o))
            .and_then(|o| o.as_array().ok())
            .cloned()
            .unwrap_or_default();
        if annots.is_empty() {
            continue;
        }
        let mut keep = Vec::new();
        let mut forms = Vec::new();
        for (n, a) in annots.iter().enumerate() {
            let Ok(dict) = deref(doc, a).as_dict().cloned() else {
                continue;
            };
            let subtype = dict
                .get(b"Subtype")
                .and_then(Object::as_name)
                .unwrap_or(b"");
            if matches!(subtype, b"Link" | b"Popup") {
                keep.push(a.clone());
                continue;
            }
            let flags = dict
                .get(b"F")
                .ok()
                .and_then(|o| num(deref(doc, o)))
                .unwrap_or(0.0) as i64;
            let hidden = flags & 0b10 != 0 || flags & 0b10_0000 != 0;
            if !hidden
                && let Some(form) = appearance(doc, &dict)
                && let Some(rect) = annot_rect(doc, &dict)
                && let Ok(stream) = doc.get_object(form).and_then(Object::as_stream)
                && let Ok(bbox) = stream.dict.get(b"BBox")
                && let Some(bbox) = rect_of(doc, bbox)
            {
                let m = placement(bbox, matrix_of(doc, &stream.dict), rect);
                forms.push((format!("DeftFlat{n}"), form, m));
            }
            total += 1;
        }
        draw_forms(doc, page, &forms)?;
        let d = doc
            .get_dictionary_mut(page)
            .map_err(|e| err(ErrorCode::InputCorrupt, e.to_string()))?;
        if keep.is_empty() {
            d.remove(b"Annots");
        } else {
            d.set("Annots", Object::Array(keep));
        }
    }
    if let Ok(catalog) = doc.catalog_mut() {
        catalog.remove(b"AcroForm");
    }
    Ok(total)
}

#[cfg(test)]
#[path = "forms_tests.rs"]
mod tests;
