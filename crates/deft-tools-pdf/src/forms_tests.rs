use super::*;
use crate::{load, save};

/// A one-page form: a comb field for the first part of an SSN, named the way
/// IRS forms name fields, a plain text field and a multi-select list box.
fn form() -> Document {
    let mut doc = Document::with_version("1.7");
    let tree = doc.new_object_id();
    let page = doc.new_object_id();
    let widget = |doc: &mut Document, extra: Dictionary| {
        let mut d = dictionary! {
            "Type" => "Annot", "Subtype" => "Widget", "P" => page,
            "Rect" => vec![10.into(), 10.into(), 100.into(), 30.into()],
        };
        d.extend(&extra);
        doc.add_object(d)
    };
    let ssn = widget(
        &mut doc,
        dictionary! {
            "T" => Object::string_literal("f1_11[0]"), "FT" => "Tx",
            "Ff" => 1 << 24, "MaxLen" => 3,
        },
    );
    let parent = doc.add_object(dictionary! {
        "T" => Object::string_literal("Page1[0]"), "Kids" => vec![ssn.into()],
    });
    doc.get_dictionary_mut(ssn).unwrap().set("Parent", parent);
    let name = widget(
        &mut doc,
        dictionary! { "T" => Object::string_literal("First Name"), "FT" => "Tx" },
    );
    let options: Vec<Object> = ["Cat", "Dog", "Fish"]
        .iter()
        .map(|o| Object::string_literal(*o))
        .collect();
    let pets = widget(
        &mut doc,
        dictionary! {
            "T" => Object::string_literal("Pets"), "FT" => "Ch", "Ff" => 1 << 21,
            "Opt" => options,
        },
    );
    let annots: Vec<Object> = vec![ssn.into(), name.into(), pets.into()];
    doc.objects.insert(
        page,
        Object::Dictionary(dictionary! {
            "Type" => "Page", "Parent" => tree, "Annots" => annots,
            "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
        }),
    );
    doc.objects.insert(
        tree,
        Object::Dictionary(
            dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 },
        ),
    );
    let form = dictionary! {
        "Fields" => vec![parent.into(), name.into(), pets.into()],
        "DA" => Object::string_literal("/Helv 0 Tf 0 g"),
    };
    let cat =
        doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => tree, "AcroForm" => form });
    doc.trailer.set("Root", cat);
    load(&save(&mut doc).unwrap(), None).unwrap()
}

fn values(pairs: &[(&str, Value)]) -> BTreeMap<String, Value> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect()
}

fn value_of(doc: &Document, name: &str) -> Option<String> {
    fields(doc).into_iter().find(|f| f.name == name)?.value
}

#[test]
fn a_value_longer_than_the_field_is_refused() {
    let mut doc = form();
    let e = fill(
        &mut doc,
        &values(&[("Page1[0].f1_11[0]", json!("123456789"))]),
    )
    .unwrap_err();
    assert_eq!(e.code, ErrorCode::BadValue);
    assert!(e.message.contains("at most 3 characters"), "{}", e.message);
    fill(&mut doc, &values(&[("Page1[0].f1_11[0]", json!("123"))])).unwrap();
    assert_eq!(value_of(&doc, "Page1[0].f1_11[0]").as_deref(), Some("123"));
}

#[test]
fn fields_report_the_maximum_length() {
    let doc = form();
    let list: Vec<Value> = fields(&doc).iter().map(Field::to_json).collect();
    assert_eq!(list[0]["max_length"], json!(3));
    assert!(list[1].get("max_length").is_none());
}

#[test]
fn a_short_name_needs_no_index() {
    let mut doc = form();
    fill(&mut doc, &values(&[("f1_11", json!("123"))])).unwrap();
    assert_eq!(value_of(&doc, "Page1[0].f1_11[0]").as_deref(), Some("123"));
    // An index that is given must match.
    assert!(fill(&mut doc, &values(&[("f1_11[1]", json!("1"))])).is_err());
}

#[test]
fn a_list_for_a_text_field_is_refused() {
    let mut doc = form();
    let e = fill(&mut doc, &values(&[("First Name", json!(["a", "b"]))])).unwrap_err();
    assert_eq!(e.code, ErrorCode::BadValue);
    assert!(e.message.contains("got a list"), "{}", e.message);
    let e = fill(&mut doc, &values(&[("First Name", json!({"a": 1}))])).unwrap_err();
    assert!(e.message.contains("got an object"), "{}", e.message);
    // A number is fine as text.
    fill(&mut doc, &values(&[("First Name", json!(42))])).unwrap();
    assert_eq!(value_of(&doc, "First Name").as_deref(), Some("42"));
}

#[test]
fn a_list_selects_options_of_a_list_box() {
    let mut doc = form();
    let warnings = fill(&mut doc, &values(&[("Pets", json!(["Cat", "Fish"]))])).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let f = fields(&doc).into_iter().find(|f| f.name == "Pets").unwrap();
    let field = doc.get_dictionary(f.id).unwrap();
    assert_eq!(field.get(b"V").unwrap().as_array().unwrap().len(), 2);
    let picked: Vec<i64> = field
        .get(b"I")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o.as_i64().unwrap())
        .collect();
    assert_eq!(picked, [0, 2]);
}

#[test]
fn a_dynamic_xfa_form_is_not_filled() {
    let mut doc = form();
    assert!(!is_dynamic_xfa(&doc));
    let xfa = doc.add_object(Stream::new(Dictionary::new(), b"<xdp/>".to_vec()));
    acro_form_mut(&mut doc).unwrap().set("XFA", xfa);
    // A static XFA form, such as an IRS form, has its fields in the PDF too.
    assert!(!is_dynamic_xfa(&doc));
    doc.catalog_mut().unwrap().set("NeedsRendering", true);
    assert!(is_dynamic_xfa(&doc));
    let e = fill(&mut doc, &values(&[("First Name", json!("Ada"))])).unwrap_err();
    assert_eq!(e.code, ErrorCode::InputUnsupported);
}

#[test]
fn flattening_drops_the_field_objects() {
    let mut doc = form();
    fill(&mut doc, &values(&[("First Name", json!("Ada"))])).unwrap();
    // A tagged form: the structure tree points at a widget.
    let widget = fields(&doc)
        .into_iter()
        .find(|f| f.name == "First Name")
        .unwrap()
        .widgets[0];
    let objr = dictionary! { "Type" => "OBJR", "Obj" => widget };
    let elem = doc.add_object(dictionary! {
        "Type" => "StructElem", "S" => "Form", "K" => vec![0.into(), Object::Dictionary(objr)],
    });
    let root = doc.add_object(dictionary! { "Type" => "StructTreeRoot", "K" => elem });
    doc.catalog_mut().unwrap().set("StructTreeRoot", root);
    flatten(&mut doc).unwrap();
    let left = doc
        .objects
        .values()
        .filter(|o| o.as_dict().is_ok_and(|d| d.has(b"FT") || d.has(b"T")))
        .count();
    assert_eq!(left, 0, "no field or widget stays in the file");
    let k = doc.get_dictionary(elem).unwrap().get(b"K").unwrap();
    assert_eq!(k.as_array().unwrap(), &vec![Object::Integer(0)]);
}
