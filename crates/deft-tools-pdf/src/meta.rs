//! Document metadata and password protection.

use crate::{deref, text_string};
use deft_core::error::{ErrorCode, Result, err};
use lopdf::encryption::crypt_filters::{Aes256CryptFilter, CryptFilter};
use lopdf::{
    Dictionary, Document, EncryptionState, EncryptionVersion, Object, ObjectId, Permissions,
    StringFormat,
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

/// The document's metadata as (name, value) pairs: the information
/// dictionary, and the XMP packet for what the dictionary lacks.
pub fn read(doc: &Document) -> Vec<(&'static str, String)> {
    let info = doc
        .trailer
        .get(b"Info")
        .ok()
        .map(|o| deref(doc, o))
        .and_then(|o| o.as_dict().ok());
    let packet = xmp_packet(doc).map(|p| p.1);
    FIELDS
        .iter()
        .filter_map(|(name, key)| {
            let text = info
                .and_then(|i| text_string(doc, i.get(key).ok()?))
                .filter(|t| !t.trim().is_empty())
                .or_else(|| crate::xmp::read(packet.as_deref()?, key))?;
            Some((*name, text))
        })
        .collect()
}

/// The catalog's XMP packet, as text.
fn xmp_packet(doc: &Document) -> Option<(ObjectId, String)> {
    let id = doc
        .catalog()
        .ok()?
        .get(b"Metadata")
        .ok()?
        .as_reference()
        .ok()?;
    let stream = doc.get_object(id).ok()?.as_stream().ok()?;
    let bytes = stream.get_plain_content().ok()?;
    let text = String::from_utf8(bytes).ok()?;
    Some((id, text.trim_start_matches('\u{FEFF}').to_string()))
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

/// Set fields, leaving the others alone, in the information dictionary and
/// the XMP packet. Returns warnings.
pub fn write(doc: &mut Document, fields: &[(&[u8], String)]) -> Result<Vec<String>> {
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
    let Some((id, packet)) = xmp_packet(doc) else {
        return Ok(vec![]);
    };
    match crate::xmp::update(&packet, fields) {
        Some(updated) => {
            if let Ok(Object::Stream(s)) = doc.get_object_mut(id) {
                s.dict.remove(b"Filter");
                s.dict.remove(b"DecodeParms");
                s.set_content(updated.into_bytes());
                // Metadata stays readable as plain text.
                s.allows_compression = false;
            }
            Ok(vec![])
        }
        None => {
            // A packet that contradicts the new values must not stay.
            drop_xmp(doc);
            Ok(vec![
                "the XMP metadata could not be read, so it was removed".to_string(),
            ])
        }
    }
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
    // Encrypted when saved, by `save_protected`.
    doc.encryption_state = Some(state);
    // AES-256 needs a newer file version than many inputs declare.
    if doc.version.as_str() < "1.7" {
        doc.version = "1.7".to_string();
    }
    Ok(())
}

/// The encryption dictionary. PDFKit opens a file whose crypt filter has no
/// key length, then shows blank pages; lopdf leaves it out. It is in bytes.
fn encryption_dictionary(state: &EncryptionState) -> Result<Dictionary> {
    let mut dict = state
        .encode()
        .map_err(|e| err(ErrorCode::Internal, format!("encryption: {e}")))?;
    if let Ok(filters) = dict.get_mut(b"CF").and_then(Object::as_dict_mut) {
        for (_, filter) in filters.iter_mut() {
            let Ok(filter) = filter.as_dict_mut() else {
                continue;
            };
            let length = match filter.get(b"CFM").and_then(Object::as_name) {
                Ok(b"AESV3") => 32,
                Ok(b"AESV2") => 16,
                _ => continue,
            };
            filter.set("Length", length);
        }
    }
    Ok(dict)
}

/// Whether the input was password-protected (and opened with its password).
pub fn is_protected(doc: &Document) -> bool {
    doc.encryption_state.is_some()
}

/// Serialise, protected with the document's encryption: the input's own
/// passwords and permissions, or those `encrypt` set. Only decryption should
/// take a password away. Object streams are kept, as in an open file.
pub fn save_protected(doc: &mut Document) -> Result<Vec<u8>> {
    let Some(state) = doc.encryption_state.clone() else {
        return crate::save(doc);
    };
    // The same file key and password entries make the same passwords open it.
    crate::writer::write_encrypted(doc, &state, encryption_dictionary(&state)?)
}

/// Decrypt the attachments of a file where only they are encrypted (Acrobat's
/// "encrypt only file attachments", which `load` reads without a password),
/// so the document needs no password at all. A /Crypt filter on any stream
/// goes too. Does nothing to other files.
pub fn decrypt_attachments(doc: &mut Document, password: Option<&str>) -> Result<()> {
    let Ok(key) = doc.trailer.get(b"Encrypt").cloned() else {
        return Ok(());
    };
    let locked = || {
        match password {
        Some(_) => err(
            ErrorCode::InputEncrypted,
            "wrong password: it does not open the encrypted attachments",
        ),
        None => err(
            ErrorCode::InputEncrypted,
            "the attachments of this PDF are encrypted, so its password cannot be removed without it",
        )
        .hint("add --password <password>; only the attachments need it"),
    }
    };
    let state = EncryptionState::decode(doc, password.unwrap_or("")).map_err(|_| locked())?;
    let dict = deref(doc, &key).as_dict().map_err(|_| locked())?;
    let name = |k: &[u8]| {
        dict.get(k)
            .ok()
            .and_then(|o| o.as_name().ok())
            .map(<[u8]>::to_vec)
    };
    // Attachments use /EFF, else the stream filter.
    let default = name(b"EFF")
        .or_else(|| name(b"StmF"))
        .unwrap_or_else(|| b"Identity".to_vec());
    let broken = || {
        err(
            ErrorCode::InputCorrupt,
            "could not decrypt the attachments of this PDF",
        )
    };
    let attached = embedded_files(doc);
    let ids: Vec<ObjectId> = doc.objects.keys().copied().collect();
    for id in ids {
        let Ok(Object::Stream(stream)) = doc.get_object_mut(id) else {
            continue;
        };
        let embedded = attached.binary_search(&id).is_ok();
        let has_crypt = stream
            .filters()
            .is_ok_and(|f| f.iter().any(|n| *n == b"Crypt"));
        if !embedded && !has_crypt {
            continue;
        }
        let filters: Vec<Vec<u8>> = stream
            .filters()
            .map(|f| f.iter().map(|n| n.to_vec()).collect())
            .unwrap_or_default();
        let crypt = filters.iter().position(|f| f == b"Crypt");
        let filter = match crypt {
            // A /Crypt filter names its own crypt filter; none means Identity.
            Some(at) => {
                let parms = match stream.dict.get(b"DecodeParms") {
                    Ok(Object::Array(a)) => a.get(at).cloned(),
                    Ok(o) => Some(o.clone()),
                    Err(_) => None,
                };
                parms
                    .and_then(|p| p.as_dict().ok().and_then(|d| d.get(b"Name").ok()).cloned())
                    .and_then(|n| n.as_name().ok().map(<[u8]>::to_vec))
                    .unwrap_or_else(|| b"Identity".to_vec())
            }
            None if embedded => default.clone(),
            // Other streams use /StmF, which is Identity in these files.
            None => b"Identity".to_vec(),
        };
        if filter != b"Identity" {
            let crypt_filter = state.crypt_filters().get(&filter).ok_or_else(broken)?;
            let k = crypt_filter
                .compute_key(state.file_encryption_key(), id)
                .map_err(|_| broken())?;
            let plain = crypt_filter
                .decrypt(&k, &stream.content)
                .map_err(|_| broken())?;
            stream.set_content(plain);
        }
        if let Some(at) = crypt {
            drop_filter(&mut stream.dict, at, filters.len());
        }
        // A wrong key gives noise, which a compressed stream shows.
        if stream
            .filters()
            .is_ok_and(|f| f.iter().any(|n| *n == b"FlateDecode"))
            && stream.decompressed_content().is_err()
        {
            return Err(broken());
        }
    }
    doc.trailer.remove(b"Encrypt");
    if let Ok(id) = key.as_reference() {
        doc.objects.remove(&id);
    }
    doc.encryption_state = None;
    Ok(())
}

/// Remove filter `at` of `count` from a stream dictionary, with its parameters.
fn drop_filter(dict: &mut Dictionary, at: usize, count: usize) {
    if count <= 1 {
        dict.remove(b"Filter");
        dict.remove(b"DecodeParms");
        return;
    }
    if let Ok(Object::Array(a)) = dict.get_mut(b"Filter") {
        a.remove(at);
    }
    match dict.get_mut(b"DecodeParms") {
        Ok(Object::Array(a)) if a.len() > at => {
            a.remove(at);
        }
        Ok(Object::Array(_)) => {}
        Ok(_) => {
            dict.remove(b"DecodeParms");
        }
        Err(_) => {}
    }
}

/// The streams holding attached files: those a file specification's /EF
/// names, and those marked /EmbeddedFile.
fn embedded_files(doc: &Document) -> Vec<ObjectId> {
    let mut out = Vec::new();
    for (&id, o) in &doc.objects {
        let dict = match o {
            Object::Dictionary(d) => d,
            Object::Stream(s) => {
                if s.dict.has_type(b"EmbeddedFile") {
                    out.push(id);
                }
                &s.dict
            }
            _ => continue,
        };
        if let Ok(ef) = dict.get(b"EF").map(|o| deref(doc, o))
            && let Ok(ef) = ef.as_dict()
        {
            out.extend(ef.iter().filter_map(|(_, v)| v.as_reference().ok()));
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

#[cfg(test)]
#[path = "meta_tests.rs"]
mod tests;
