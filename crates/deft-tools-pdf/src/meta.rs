//! Document metadata and password protection.

use crate::{deref, text_string};
use deft_core::error::{ErrorCode, Result, err};
use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
use lopdf::{
    Dictionary, Document, EncryptionState, EncryptionVersion, Object, Permissions, StringFormat,
};
use std::collections::BTreeMap;
use std::sync::Arc;

pub const FIELDS: &[(&str, &[u8])] = &[
    ("title", b"Title"),
    ("author", b"Author"),
    ("subject", b"Subject"),
    ("keywords", b"Keywords"),
    ("creator", b"Creator"),
    ("producer", b"Producer"),
    ("created", b"CreationDate"),
    ("modified", b"ModDate"),
];

/// The document information dictionary as (name, value) pairs.
pub fn read(doc: &Document) -> Vec<(&'static str, String)> {
    let info = doc
        .trailer
        .get(b"Info")
        .ok()
        .map(|o| deref(doc, o))
        .and_then(|o| o.as_dict().ok());
    let Some(info) = info else {
        return vec![];
    };
    FIELDS
        .iter()
        .filter_map(|(name, key)| {
            let text = text_string(doc, info.get(key).ok()?)?;
            (!text.trim().is_empty()).then_some((*name, text))
        })
        .collect()
}

/// A text string: PDFDocEncoding for ASCII, UTF-16 for anything else.
pub(crate) fn text_object(text: &str) -> Object {
    if text.is_ascii() {
        return Object::String(text.as_bytes().to_vec(), StringFormat::Literal);
    }
    let mut bytes = vec![0xFE, 0xFF];
    for unit in text.encode_utf16() {
        bytes.extend_from_slice(&unit.to_be_bytes());
    }
    Object::String(bytes, StringFormat::Hexadecimal)
}

/// The object goes too: lopdf writes unreferenced objects, packet and all.
fn drop_xmp(doc: &mut Document) {
    if let Ok(catalog) = doc.catalog_mut() {
        catalog.remove(b"Metadata");
    }
    crate::prune(doc);
}

/// Set fields, leaving the others alone. An XMP packet would contradict the
/// new values, so it goes.
pub fn write(doc: &mut Document, fields: &[(&[u8], String)]) -> Result<()> {
    let info_id = match doc.trailer.get(b"Info").and_then(Object::as_reference) {
        Ok(id) => id,
        Err(_) => {
            let id = doc.add_object(Dictionary::new());
            doc.trailer.set("Info", Object::Reference(id));
            id
        }
    };
    let info = doc
        .get_dictionary_mut(info_id)
        .map_err(|e| err(ErrorCode::InputCorrupt, e.to_string()))?;
    for (key, value) in fields {
        info.set(key.to_vec(), text_object(value));
    }
    drop_xmp(doc);
    Ok(())
}

/// Take `word` out of what repeats page text outside the pages: the information
/// dictionary, the XMP packet and bookmark titles. Returns how many places held it.
pub fn scrub(doc: &mut Document, word: &str) -> usize {
    let wanted = word.to_lowercase();
    let holds = |text: &str| text.to_lowercase().contains(&wanted);
    let mut found = 0;
    let info = doc.trailer.get(b"Info").and_then(Object::as_reference);
    if let Ok(id) = info {
        let keys: Vec<Vec<u8>> = doc
            .get_dictionary(id)
            .map(|d| {
                d.iter()
                    .filter(|(_, v)| text_string(doc, v).is_some_and(|t| holds(&t)))
                    .map(|(k, _)| k.clone())
                    .collect()
            })
            .unwrap_or_default();
        found += keys.len();
        if let Ok(d) = doc.get_dictionary_mut(id) {
            keys.iter().for_each(|k| {
                d.remove(k);
            });
        }
    }
    let packet = doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"Metadata").ok())
        .and_then(|o| deref(doc, o).as_stream().ok())
        .map(|s| {
            s.decompressed_content()
                .unwrap_or_else(|_| s.content.clone())
        });
    if packet.is_some_and(|p| holds(&String::from_utf8_lossy(&p))) {
        found += 1;
        drop_xmp(doc);
    }
    for id in crate::outline::all_items(doc) {
        let title = doc
            .get_dictionary(id)
            .ok()
            .and_then(|d| d.get(b"Title").ok())
            .and_then(|o| text_string(doc, o));
        if title.is_some_and(|t| holds(&t))
            && let Ok(d) = doc.get_dictionary_mut(id)
        {
            d.set("Title", text_object("Redacted"));
            found += 1;
        }
    }
    found
}

/// Encrypt with AES-256. `owner` falls back to `user`; printing and copying
/// stay allowed unless the matching flag is set.
pub fn encrypt(
    doc: &mut Document,
    user: &str,
    owner: Option<&str>,
    no_print: bool,
    no_copy: bool,
) -> Result<()> {
    let mut key = [0u8; 32];
    getrandom::fill(&mut key).map_err(|e| err(ErrorCode::Internal, e.to_string()))?;
    let filter: Arc<dyn CryptFilter> = Arc::new(Aes256CryptFilter);
    let mut filters = BTreeMap::new();
    filters.insert(b"StdCF".to_vec(), filter);
    let mut permissions = Permissions::all();
    if no_print {
        permissions.remove(Permissions::PRINTABLE | Permissions::PRINTABLE_IN_HIGH_QUALITY);
    }
    if no_copy {
        permissions.remove(Permissions::COPYABLE | Permissions::COPYABLE_FOR_ACCESSIBILITY);
    }
    let version = EncryptionVersion::V5 {
        encrypt_metadata: true,
        crypt_filters: filters,
        file_encryption_key: &key,
        stream_filter: b"StdCF".to_vec(),
        string_filter: b"StdCF".to_vec(),
        owner_password: owner.unwrap_or(user),
        user_password: user,
        permissions,
    };
    let state = EncryptionState::try_from(version)
        .map_err(|e| err(ErrorCode::Internal, format!("encryption: {e}")))?;
    doc.encrypt(&state)
        .map_err(|e| err(ErrorCode::Internal, format!("encryption: {e}")))?;
    state_key_length(doc);
    // AES-256 needs a newer file version than many inputs declare.
    if doc.version.as_str() < "1.7" {
        doc.version = "1.7".to_string();
    }
    // Encrypted strings must not move into object streams.
    Ok(())
}

/// PDFKit opens a file whose crypt filter has no key length, then shows blank
/// pages. lopdf leaves it out. It is in bytes here.
fn state_key_length(doc: &mut Document) {
    let Ok(id) = doc.trailer.get(b"Encrypt").and_then(Object::as_reference) else {
        return;
    };
    let filter = doc
        .get_dictionary_mut(id)
        .and_then(|d| d.get_mut(b"CF"))
        .and_then(Object::as_dict_mut)
        .and_then(|cf| cf.get_mut(b"StdCF"))
        .and_then(Object::as_dict_mut);
    if let Ok(filter) = filter {
        filter.set("Length", 32);
    }
}

/// Serialise an encrypted document, which cannot use object streams.
pub fn save_encrypted(doc: &mut Document) -> Result<Vec<u8>> {
    let mut out = Vec::new();
    doc.save_to(&mut out)
        .map_err(|e| err(ErrorCode::Internal, format!("writing PDF: {e}")))?;
    Ok(out)
}
