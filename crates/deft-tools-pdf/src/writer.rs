//! Writing an encrypted PDF with object streams. lopdf writes an encrypted
//! document without them, every small object on its own, which made a
//! protected copy of a long document three times its size.

use deft_core::error::{ErrorCode, Result, err};
use lopdf::encryption::encrypt_object;
use lopdf::{Dictionary, Document, EncryptionState, Object, ObjectId, ObjectStream, Stream};
use std::io::Write;

/// Objects per object stream.
const PER_STREAM: usize = 100;

fn name(out: &mut Vec<u8>, n: &[u8]) {
    out.push(b'/');
    for &b in n {
        let regular = (b'!'..=b'~').contains(&b) && !b"()<>[]{}/%#".contains(&b);
        if regular {
            out.push(b);
        } else {
            let _ = write!(out, "#{b:02X}");
        }
    }
}

/// One object in PDF syntax. Strings go out in hex, which holds any bytes.
fn object(out: &mut Vec<u8>, o: &Object) {
    match o {
        Object::Null => out.extend_from_slice(b"null"),
        Object::Boolean(b) => out.extend_from_slice(if *b { b"true" } else { b"false" }),
        Object::Integer(i) => {
            let _ = write!(out, "{i}");
        }
        Object::Real(r) => {
            let r = if r.is_finite() { *r } else { 0.0 };
            let _ = write!(out, "{r}");
        }
        Object::Name(n) => name(out, n),
        Object::String(s, _) => {
            out.push(b'<');
            for b in s {
                let _ = write!(out, "{b:02X}");
            }
            out.push(b'>');
        }
        Object::Array(a) => {
            out.push(b'[');
            for (i, x) in a.iter().enumerate() {
                if i > 0 {
                    out.push(b' ');
                }
                object(out, x);
            }
            out.push(b']');
        }
        Object::Dictionary(d) => dictionary(out, d),
        Object::Stream(s) => {
            let mut dict = s.dict.clone();
            dict.set("Length", s.content.len() as i64);
            dictionary(out, &dict);
            out.extend_from_slice(b"\nstream\n");
            out.extend_from_slice(&s.content);
            out.extend_from_slice(b"\nendstream");
        }
        Object::Reference((n, g)) => {
            let _ = write!(out, "{n} {g} R");
        }
    }
}

fn dictionary(out: &mut Vec<u8>, d: &Dictionary) {
    out.extend_from_slice(b"<<");
    for (k, v) in d.iter() {
        name(out, k);
        out.push(b' ');
        object(out, v);
    }
    out.extend_from_slice(b">>");
}

enum Entry {
    At(usize, u16),
    Packed(u32, u16),
}

/// The document, encrypted with `state`, with its small objects in object
/// streams. Objects in an object stream are not encrypted on their own; the
/// stream is (ISO 32000-1, 7.5.7). `encrypt` is the encryption dictionary.
pub(crate) fn write_encrypted(
    doc: &Document,
    state: &EncryptionState,
    encrypt: Dictionary,
) -> Result<Vec<u8>> {
    let fail = |e: lopdf::Error| err(ErrorCode::Internal, format!("writing PDF: {e}"));
    // Streams the writer makes again, and a stale linearization dictionary.
    let stale = |o: &Object| {
        o.type_name().is_ok_and(|t| t == b"ObjStm" || t == b"XRef")
            || o.as_dict().is_ok_and(|d| d.has(b"Linearized"))
    };
    let mut loose: Vec<(ObjectId, Object)> = Vec::new();
    let mut packed: Vec<(ObjectId, Object)> = Vec::new();
    for (&id, o) in &doc.objects {
        if stale(o) {
            continue;
        }
        if id.1 == 0 && !matches!(o, Object::Stream(_)) {
            packed.push((id, o.clone()));
        } else {
            loose.push((id, o.clone()));
        }
    }
    let top = doc.objects.keys().map(|id| id.0).max().unwrap_or(0);
    let mut next = doc.max_id.max(top) + 1;
    let mut entries: Vec<(u32, Entry)> = Vec::new();
    let mut containers = Vec::new();
    for chunk in packed.chunks(PER_STREAM) {
        let mut stream = ObjectStream::builder()
            .max_objects(PER_STREAM)
            .compression_level(6)
            .build();
        for (id, o) in chunk {
            stream.add_object(*id, o.clone()).map_err(fail)?;
        }
        let container = next;
        next += 1;
        // Members are stored in object number order.
        let mut ids: Vec<ObjectId> = chunk.iter().map(|c| c.0).collect();
        ids.sort_unstable();
        for (index, id) in ids.iter().enumerate() {
            entries.push((id.0, Entry::Packed(container, index as u16)));
        }
        let content = stream.to_stream_object().map_err(fail)?;
        containers.push(((container, 0), Object::Stream(content)));
    }
    let encrypt_id = (next, 0);
    next += 1;
    let mut out = Vec::new();
    let _ = write!(out, "%PDF-{}\n%\u{e2}\u{e3}\u{cf}\u{d3}\n", doc.version);
    for (id, mut o) in loose.into_iter().chain(containers) {
        encrypt_object(state, id, &mut o)
            .map_err(|e| err(ErrorCode::Internal, format!("encryption: {e}")))?;
        entries.push((id.0, Entry::At(out.len(), id.1)));
        let _ = writeln!(out, "{} {} obj", id.0, id.1);
        object(&mut out, &o);
        out.extend_from_slice(b"\nendobj\n");
    }
    entries.push((encrypt_id.0, Entry::At(out.len(), 0)));
    let _ = writeln!(out, "{} 0 obj", encrypt_id.0);
    dictionary(&mut out, &encrypt);
    out.extend_from_slice(b"\nendobj\n");
    // The cross-reference stream, which is never encrypted.
    let xref_id = next;
    let size = xref_id + 1;
    let start = out.len();
    entries.push((xref_id, Entry::At(start, 0)));
    let mut rows = vec![0u8; size as usize * 7];
    rows[0..7].copy_from_slice(&[0, 0, 0, 0, 0, 0xFF, 0xFF]);
    for (n, entry) in entries {
        let row = &mut rows[n as usize * 7..n as usize * 7 + 7];
        match entry {
            Entry::At(offset, generation) => {
                row[0] = 1;
                row[1..5].copy_from_slice(&(offset as u32).to_be_bytes());
                row[5..7].copy_from_slice(&generation.to_be_bytes());
            }
            Entry::Packed(container, index) => {
                row[0] = 2;
                row[1..5].copy_from_slice(&container.to_be_bytes());
                row[5..7].copy_from_slice(&index.to_be_bytes());
            }
        }
    }
    let mut trailer = Dictionary::new();
    for key in [&b"Root"[..], b"Info", b"ID"] {
        if let Ok(v) = doc.trailer.get(key) {
            trailer.set(key.to_vec(), v.clone());
        }
    }
    trailer.set("Encrypt", Object::Reference(encrypt_id));
    trailer.set("Type", "XRef");
    trailer.set("Size", size as i64);
    trailer.set(
        "W",
        vec![Object::Integer(1), Object::Integer(4), Object::Integer(2)],
    );
    trailer.set("Filter", "FlateDecode");
    let xref = Stream::new(trailer, crate::flate(&rows));
    let _ = writeln!(out, "{xref_id} 0 obj");
    object(&mut out, &Object::Stream(xref));
    let _ = write!(out, "\nendobj\nstartxref\n{start}\n%%EOF\n");
    Ok(out)
}
