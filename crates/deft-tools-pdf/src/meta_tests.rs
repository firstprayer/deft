use super::*;
use crate::{load, page_ids, page_rotation, rotate, save};
use lopdf::{Stream, dictionary};

fn plain_pdf(pages: usize) -> Document {
    let mut doc = Document::with_version("1.7");
    let tree = doc.new_object_id();
    let mut kids = vec![];
    for i in 0..pages {
        let text = format!("BT /F1 12 Tf 50 50 Td (secret {i}) Tj ET");
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
    let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => tree });
    doc.trailer.set("Root", cat);
    doc.trailer.set(
        "ID",
        vec![
            Object::string_literal(b"0123456789abcdef".to_vec()),
            Object::string_literal(b"0123456789abcdef".to_vec()),
        ],
    );
    doc
}

fn aes_locked() -> Vec<u8> {
    let mut doc = plain_pdf(2);
    encrypt(&mut doc, "user", Some("owner"), true, false).unwrap();
    save_encrypted(&mut doc).unwrap()
}

/// RC4 with a 128-bit key, whose file key depends on the file ID.
fn rc4_locked() -> Vec<u8> {
    let mut doc = plain_pdf(2);
    let state = EncryptionState::try_from(EncryptionVersion::V2 {
        document: &doc,
        owner_password: "owner",
        user_password: "user",
        key_length: 128,
        permissions: Permissions::all() - Permissions::MODIFIABLE,
    })
    .unwrap();
    doc.encrypt(&state).unwrap();
    save_encrypted(&mut doc).unwrap()
}

/// The /P entry of the encryption dictionary, read from the file text.
fn permissions(bytes: &[u8]) -> i64 {
    let at = bytes.windows(3).position(|w| w == b"/P ").unwrap() + 3;
    let text: String = bytes[at..]
        .iter()
        .take_while(|b| b.is_ascii_digit() || **b == b'-')
        .map(|b| *b as char)
        .collect();
    text.parse().unwrap()
}

#[test]
fn a_protected_input_stays_protected() {
    for locked in [aes_locked(), rc4_locked()] {
        let mut doc = load(&locked, Some("user")).unwrap();
        assert!(is_protected(&doc));
        rotate(&mut doc, &[0], 90);
        let out = save_protected(&mut doc).unwrap();
        let e = load(&out, None).unwrap_err();
        assert_eq!(e.code, ErrorCode::InputEncrypted);
        for password in ["user", "owner"] {
            let doc = load(&out, Some(password)).unwrap();
            assert_eq!(page_rotation(&doc, page_ids(&doc)[0]), 90);
        }
    }
}

#[test]
fn the_permissions_stay_as_they_were() {
    let locked = rc4_locked();
    let mut doc = load(&locked, Some("owner")).unwrap();
    let out = save_protected(&mut doc).unwrap();
    assert_eq!(permissions(&out), permissions(&locked));
    assert_eq!(permissions(&out) & 0b1000, 0, "modifying stays forbidden");
}

#[test]
fn a_merge_with_a_protected_input_is_protected() {
    // A different file ID: the RC4 key of the other input depends on its own.
    let mut open = plain_pdf(1);
    let other = Object::string_literal(b"fedcba9876543210".to_vec());
    open.trailer.set("ID", vec![other.clone(), other]);
    let open = load(&save(&mut open).unwrap(), None).unwrap();
    let locked = load(&rc4_locked(), Some("user")).unwrap();
    let mut merged = crate::merge(vec![open, locked]).unwrap();
    let out = save_protected(&mut merged).unwrap();
    assert!(load(&out, None).is_err());
    assert_eq!(page_ids(&load(&out, Some("user")).unwrap()).len(), 3);
}

#[test]
fn an_unprotected_input_is_saved_as_it_was() {
    let mut doc = plain_pdf(1);
    let mut doc = load(&save(&mut doc).unwrap(), None).unwrap();
    assert!(!is_protected(&doc));
    let out = save_protected(&mut doc).unwrap();
    assert!(load(&out, None).is_ok());
}
