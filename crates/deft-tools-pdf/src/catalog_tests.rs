use super::*;
use crate::{load, merge, page_ids, save, select};
use lopdf::Stream;

/// A document of `pages` pages; `extra` adds entries to its catalog.
fn doc_with(pages: usize, extra: impl FnOnce(&mut Document) -> Dictionary) -> Document {
    let mut doc = Document::with_version("1.7");
    let tree = doc.new_object_id();
    let mut kids = vec![];
    for i in 0..pages {
        let text = format!("BT /F1 12 Tf 50 50 Td (page {i}) Tj ET");
        let c = doc.add_object(Stream::new(Dictionary::new(), text.into_bytes()));
        let p = doc.add_object(dictionary! {
            "Type" => "Page", "Parent" => tree, "Contents" => c,
            "MediaBox" => vec![0.into(), 0.into(), 200.into(), 200.into()],
        });
        kids.push(Object::Reference(p));
    }
    let n = kids.len() as i64;
    doc.objects.insert(
        tree,
        Object::Dictionary(dictionary! { "Type" => "Pages", "Kids" => kids, "Count" => n }),
    );
    let mut cat = extra(&mut doc);
    cat.set("Type", "Catalog");
    cat.set("Pages", tree);
    let cat = doc.add_object(cat);
    doc.trailer.set("Root", cat);
    load(&save(&mut doc).unwrap(), None).unwrap()
}

fn attached(name: &str) -> impl FnOnce(&mut Document) -> Dictionary {
    let name = name.to_string();
    move |doc| {
        let file = doc.add_object(Stream::new(
            dictionary! { "Type" => "EmbeddedFile" },
            b"hello".to_vec(),
        ));
        let spec = doc.add_object(dictionary! {
            "Type" => "Filespec", "F" => Object::string_literal(name.clone()),
            "EF" => dictionary! { "F" => file },
        });
        let tree = doc.add_object(dictionary! {
            "Names" => vec![Object::string_literal(name), spec.into()],
        });
        dictionary! { "Names" => dictionary! { "EmbeddedFiles" => tree } }
    }
}

fn names(doc: &Document) -> Vec<String> {
    attachments(doc)
        .iter()
        .map(|(k, _)| String::from_utf8_lossy(k.as_str().unwrap()).to_string())
        .collect()
}

fn reload(mut doc: Document) -> Document {
    load(&save(&mut doc).unwrap(), None).unwrap()
}

#[test]
fn removing_pages_keeps_the_attachments() {
    let mut doc = doc_with(3, attached("notes.txt"));
    select(&mut doc, &[0, 2]).unwrap();
    assert_eq!(names(&reload(doc)), ["notes.txt"]);
}

#[test]
fn a_merge_keeps_the_attachments_of_every_input() {
    let a = doc_with(1, |_| Dictionary::new());
    let b = doc_with(1, attached("data.csv"));
    let c = doc_with(1, attached("data.csv"));
    let doc = reload(merge(vec![a, b, c]).unwrap());
    assert_eq!(names(&doc), ["data.csv", "data.csv (2)"]);
}

/// Pages numbered i, ii, 1, 2, A-1.
fn labelled(doc: &mut Document) -> Dictionary {
    let nums = vec![
        0.into(),
        dictionary! { "S" => "r" }.into(),
        2.into(),
        dictionary! { "S" => "D" }.into(),
        4.into(),
        dictionary! { "S" => "D", "P" => Object::string_literal("A-") }.into(),
    ];
    let tree = doc.add_object(dictionary! { "Nums" => nums });
    dictionary! { "PageLabels" => tree }
}

fn shown(doc: &Document) -> Vec<String> {
    let labels = page_labels(doc, page_ids(doc).len()).unwrap();
    labels
        .iter()
        .map(|l| {
            let prefix = String::from_utf8_lossy(l.prefix.as_deref().unwrap_or(b"")).to_string();
            let style = String::from_utf8_lossy(l.style.as_deref().unwrap_or(b"")).to_string();
            format!("{prefix}{style}{}", l.number)
        })
        .collect()
}

#[test]
fn pages_keep_their_labels_when_others_are_removed() {
    let mut doc = doc_with(5, labelled);
    assert_eq!(shown(&doc), ["r1", "r2", "D1", "D2", "A-D1"]);
    select(&mut doc, &[1, 3, 4]).unwrap();
    assert_eq!(shown(&reload(doc)), ["r2", "D2", "A-D1"]);
}

#[test]
fn a_merge_keeps_labels_and_numbers_the_pages_without() {
    let a = doc_with(2, |_| Dictionary::new());
    let b = doc_with(5, labelled);
    let doc = reload(merge(vec![a, b]).unwrap());
    assert_eq!(shown(&doc), ["D1", "D2", "r1", "r2", "D1", "D2", "A-D1"]);
}

fn layered(name: &'static str, off: bool) -> impl FnOnce(&mut Document) -> Dictionary {
    move |doc| {
        let group = doc.add_object(dictionary! {
            "Type" => "OCG", "Name" => Object::string_literal(name),
        });
        let mut view = dictionary! { "Order" => vec![group.into()] };
        if off {
            view.set("OFF", vec![Object::Reference(group)]);
        }
        dictionary! { "OCProperties" => dictionary! { "OCGs" => vec![group.into()], "D" => view } }
    }
}

#[test]
fn a_merge_keeps_the_layers_of_every_input() {
    let a = doc_with(1, layered("Roads", false));
    let b = doc_with(1, layered("Rivers", true));
    let doc = reload(merge(vec![a, b]).unwrap());
    let props = layers(&doc).unwrap();
    let list = |d: &Dictionary, k: &[u8]| d.get(k).unwrap().as_array().unwrap().len();
    assert_eq!(list(&props, b"OCGs"), 2);
    let view = props.get(b"D").unwrap().as_dict().unwrap();
    assert_eq!(list(view, b"Order"), 2);
    assert_eq!(list(view, b"OFF"), 1, "the hidden layer stays hidden");
}
