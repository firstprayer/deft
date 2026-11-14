//! Reading and writing EXIF metadata.

use deft_core::error::{ErrorCode, Result, err};
use deft_core::format::Format;
use serde_json::{Map, Value, json};

fn degrees(field: &exif::Field) -> Option<f64> {
    let exif::Value::Rational(r) = &field.value else {
        return None;
    };
    let part = |i: usize| r.get(i).map_or(0.0, |x| x.to_f64());
    Some(part(0) + part(1) / 60.0 + part(2) / 3600.0)
}

/// A TIFF whose Exif or GPS pointer has the IFD type (13) instead of LONG, as
/// some cameras write it, patched to LONG; the reader skips such pointers and
/// with them every Exif and GPS tag. None when nothing needs patching.
pub(crate) fn long_pointers(bytes: &[u8]) -> Option<Vec<u8>> {
    let big = match bytes.get(..4)? {
        b"MM\0\x2A" => true,
        b"II\x2A\0" => false,
        _ => return None,
    };
    let u16_at = |i: usize| -> Option<u16> {
        let b: [u8; 2] = bytes.get(i..i + 2)?.try_into().ok()?;
        Some(if big {
            u16::from_be_bytes(b)
        } else {
            u16::from_le_bytes(b)
        })
    };
    let u32_at = |i: usize| -> Option<u32> {
        let b: [u8; 4] = bytes.get(i..i + 4)?.try_into().ok()?;
        Some(if big {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        })
    };
    let ifd = u32_at(4)? as usize;
    let fix: Vec<usize> = (0..u16_at(ifd)?.min(1024) as usize)
        .map(|k| ifd + 2 + k * 12)
        .filter(|&e| matches!(u16_at(e), Some(0x8769 | 0x8825)) && u16_at(e + 2) == Some(13))
        .collect();
    if fix.is_empty() {
        return None;
    }
    let mut out = bytes.to_vec();
    let long: [u8; 2] = if big { [0, 4] } else { [4, 0] };
    for e in fix {
        out[e + 2..e + 4].copy_from_slice(&long);
    }
    Some(out)
}

fn parse(bytes: &[u8]) -> Option<exif::Exif> {
    let patched = long_pointers(bytes);
    let bytes = patched.as_deref().unwrap_or(bytes);
    // One odd tag should not hide the rest, such as a location.
    let mut reader = exif::Reader::new();
    reader.continue_on_error(true);
    let partial = |e: exif::Error| e.distill_partial_result(|_| {});
    reader
        .read_from_container(&mut std::io::Cursor::new(bytes))
        .or_else(partial)
        .ok()
        .filter(|data| data.fields().next().is_some())
        .or_else(|| {
            // Many writers start the WebP chunk with the prefix JPEG uses.
            let raw = crate::webp::exif_chunk(bytes)?;
            let tiff = raw.strip_prefix(b"Exif\0\0").unwrap_or(raw);
            reader.read_raw(tiff.to_vec()).or_else(partial).ok()
        })
}

/// Text fields are often UTF-8, which `display_value` escapes.
fn text_of(field: &exif::Field, data: &exif::Exif) -> String {
    match &field.value {
        exif::Value::Ascii(parts) if parts.iter().flatten().any(|b| !b.is_ascii()) => parts
            .iter()
            .map(|p| String::from_utf8_lossy(p).to_string())
            .collect::<Vec<_>>()
            .join(", "),
        exif::Value::Undefined(raw, _) if field.tag == exif::Tag::UserComment => {
            let text = raw.strip_prefix(b"ASCII\0\0\0").unwrap_or(raw);
            String::from_utf8_lossy(text).to_string()
        }
        _ => field.display_value().with_unit(data).to_string(),
    }
}

/// Every EXIF tag as text, plus `latitude` and `longitude` when present.
pub fn read(bytes: &[u8]) -> Map<String, Value> {
    let mut out = Map::new();
    let Some(data) = parse(bytes) else {
        return out;
    };
    for field in data.fields() {
        if field.ifd_num != exif::In::PRIMARY {
            continue;
        }
        let text = text_of(field, &data);
        let text = text.trim_matches('"').trim().to_string();
        if text.len() <= 200 && !text.is_empty() {
            out.insert(field.tag.to_string(), json!(text));
        }
    }
    let gps = |tag, reference: exif::Tag, negative: &str| {
        let value = degrees(data.get_field(tag, exif::In::PRIMARY)?)?;
        let side = data
            .get_field(reference, exif::In::PRIMARY)
            .map(|f| f.display_value().to_string())
            .unwrap_or_default();
        Some(if side.contains(negative) {
            -value
        } else {
            value
        })
    };
    if let (Some(lat), Some(lon)) = (
        gps(exif::Tag::GPSLatitude, exif::Tag::GPSLatitudeRef, "S"),
        gps(exif::Tag::GPSLongitude, exif::Tag::GPSLongitudeRef, "W"),
    ) {
        out.insert("latitude".into(), json!(lat));
        out.insert("longitude".into(), json!(lon));
    }
    out
}

/// Fields `write` can set.
#[derive(Clone, Debug, Default)]
pub struct Fields {
    pub title: Option<String>,
    pub author: Option<String>,
    pub copyright: Option<String>,
    pub comment: Option<String>,
    /// `YYYY:MM:DD HH:MM:SS`.
    pub date: Option<String>,
}

fn ascii(tag: exif::Tag, text: &str) -> exif::Field {
    exif::Field {
        tag,
        ifd_num: exif::In::PRIMARY,
        value: exif::Value::Ascii(vec![text.as_bytes().to_vec()]),
    }
}

/// Pointers and thumbnails are rebuilt or dropped by the writer.
fn keep(f: &exif::Field) -> bool {
    use exif::Tag;
    f.ifd_num == exif::In::PRIMARY
        && ![
            Tag::ExifIFDPointer,
            Tag::GPSInfoIFDPointer,
            Tag::InteropIFDPointer,
        ]
        .contains(&f.tag)
}

/// The EXIF block as a TIFF stream: the old fields plus the new ones.
fn tiff(bytes: &[u8], fields: &Fields, orientation: bool) -> Result<Vec<u8>> {
    use exif::{Tag, Value};
    let mut new = Vec::new();
    for (tag, value) in [
        (Tag::ImageDescription, &fields.title),
        (Tag::Artist, &fields.author),
        (Tag::Copyright, &fields.copyright),
        (Tag::DateTimeOriginal, &fields.date),
    ] {
        if let Some(v) = value {
            new.push(ascii(tag, v));
        }
    }
    if let Some(v) = &fields.comment {
        let mut raw = b"ASCII\0\0\0".to_vec();
        raw.extend_from_slice(v.as_bytes());
        new.push(exif::Field {
            tag: Tag::UserComment,
            ifd_num: exif::In::PRIMARY,
            value: Value::Undefined(raw, 0),
        });
    }
    let old: Vec<exif::Field> = parse(bytes)
        .map(|e| e.fields().filter(|f| keep(f)).cloned().collect())
        .unwrap_or_default();
    let kept: Vec<&exif::Field> = old
        .iter()
        .filter(|f| !new.iter().any(|n| n.tag == f.tag))
        .filter(|f| orientation || f.tag != Tag::Orientation)
        .collect();
    let mut writer = exif::experimental::Writer::new();
    for f in kept.into_iter().chain(&new) {
        writer.push_field(f);
    }
    let mut out = std::io::Cursor::new(Vec::new());
    writer
        .write(&mut out, false)
        .map_err(|e| err(ErrorCode::Internal, format!("writing metadata: {e}")))?;
    Ok(out.into_inner())
}

/// Swap the APP1 EXIF segment of a JPEG.
fn into_jpeg(bytes: &[u8], block: &[u8]) -> Result<Vec<u8>> {
    let size = block.len() + 8;
    if size > 0xFFFF {
        return Err(err(
            ErrorCode::InputUnsupported,
            "metadata too large for JPEG",
        ));
    }
    let mut out = bytes[..2].to_vec();
    out.extend_from_slice(&[0xFF, 0xE1]);
    out.extend_from_slice(&(size as u16).to_be_bytes());
    out.extend_from_slice(b"Exif\0\0");
    out.extend_from_slice(block);
    let mut i = 2;
    while i + 4 <= bytes.len() && bytes[i] == 0xFF && bytes[i + 1] != 0xDA {
        let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        let end = (i + 2 + len).min(bytes.len());
        let old_exif = bytes[i + 1] == 0xE1 && bytes[i + 4..end].starts_with(b"Exif\0\0");
        if !old_exif {
            out.extend_from_slice(&bytes[i..end]);
        }
        i = end;
    }
    out.extend_from_slice(&bytes[i..]);
    Ok(out)
}

/// Swap the eXIf chunk of a PNG, placing it right after the header chunk.
pub(crate) fn into_png(bytes: &[u8], block: &[u8]) -> Vec<u8> {
    let mut chunk = (block.len() as u32).to_be_bytes().to_vec();
    chunk.extend_from_slice(b"eXIf");
    chunk.extend_from_slice(block);
    let mut h = crc32fast::Hasher::new();
    h.update(&chunk[4..]);
    chunk.extend_from_slice(&h.finalize().to_be_bytes());
    let mut out = bytes[..8].to_vec();
    let mut i = 8;
    while i + 12 <= bytes.len() {
        let len = u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]) as usize;
        let end = (i + 12 + len).min(bytes.len());
        let kind = &bytes[i + 4..i + 8];
        if kind != b"eXIf" {
            out.extend_from_slice(&bytes[i..end]);
        }
        if kind == b"IHDR" {
            out.extend_from_slice(&chunk);
        }
        i = end;
    }
    out
}

fn into_webp(bytes: &[u8], block: &[u8]) -> Result<Vec<u8>> {
    crate::webp::with_exif(bytes, block)
        .ok_or_else(|| err(ErrorCode::InputCorrupt, "the WebP file is damaged"))
}

/// Copy the EXIF of `from` into `to`, an encoded image of the given format.
/// Orientation stays behind, because decoded pixels are already upright.
pub fn copy(from: &[u8], to: &[u8], format: Format) -> Result<Vec<u8>> {
    let block = tiff(from, &Fields::default(), false)?;
    match format {
        Format::Jpeg => into_jpeg(to, &block),
        Format::Png => Ok(into_png(to, &block)),
        Format::Webp => into_webp(to, &block),
        other => Err(err(
            ErrorCode::InputUnsupported,
            format!("cannot write metadata into {other}"),
        )),
    }
}

/// Set EXIF fields, leaving the pixels and other tags alone. An embedded
/// thumbnail is dropped, because it no longer has a place to live.
pub fn write(bytes: &[u8], format: Format, fields: &Fields) -> Result<Vec<u8>> {
    if format == Format::Tiff {
        return tiff_file::set(bytes, fields);
    }
    let block = tiff(bytes, fields, true)?;
    match format {
        Format::Jpeg => into_jpeg(bytes, &block),
        Format::Png => Ok(into_png(bytes, &block)),
        Format::Webp => into_webp(bytes, &block),
        other => Err(err(
            ErrorCode::InputUnsupported,
            format!("cannot write metadata into {other}"),
        )),
    }
}

/// Setting tags in a TIFF file itself. The tags live in the same structure
/// as the picture, so the new directories are added at the end of the file
/// and the header pointed at them; every byte of the picture stays put.
mod tiff_file {
    use super::Fields;
    use deft_core::error::{ErrorCode, Result, err};

    /// One directory entry: tag, type, count and the value's bytes.
    struct Entry {
        tag: u16,
        kind: u16,
        count: u32,
        /// The value itself when it has 4 bytes or fewer, else None and
        /// `raw` holds the 4-byte offset of the value in the file.
        value: Option<Vec<u8>>,
        raw: [u8; 4],
    }

    struct File<'a> {
        bytes: &'a [u8],
        big: bool,
    }

    impl File<'_> {
        fn u16(&self, at: usize) -> Option<u16> {
            let b: [u8; 2] = self.bytes.get(at..at + 2)?.try_into().ok()?;
            Some(if self.big {
                u16::from_be_bytes(b)
            } else {
                u16::from_le_bytes(b)
            })
        }
        fn u32(&self, at: usize) -> Option<u32> {
            let b: [u8; 4] = self.bytes.get(at..at + 4)?.try_into().ok()?;
            Some(if self.big {
                u32::from_be_bytes(b)
            } else {
                u32::from_le_bytes(b)
            })
        }
        fn put16(&self, v: u16) -> [u8; 2] {
            if self.big {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            }
        }
        fn put32(&self, v: u32) -> [u8; 4] {
            if self.big {
                v.to_be_bytes()
            } else {
                v.to_le_bytes()
            }
        }
        /// The entries of the directory at `at`, and the offset of the next one.
        fn directory(&self, at: usize) -> Option<(Vec<Entry>, u32)> {
            let n = self.u16(at)? as usize;
            let mut out = Vec::with_capacity(n);
            for k in 0..n {
                let e = at + 2 + k * 12;
                out.push(Entry {
                    tag: self.u16(e)?,
                    kind: self.u16(e + 2)?,
                    count: self.u32(e + 4)?,
                    value: None,
                    raw: self.bytes.get(e + 8..e + 12)?.try_into().ok()?,
                });
            }
            Some((out, self.u32(at + 2 + n * 12)?))
        }
    }

    /// Set a tag to ASCII text (type 2) or bytes (type 7), counted in bytes.
    fn set_entry(list: &mut Vec<Entry>, tag: u16, kind: u16, data: Vec<u8>) {
        list.retain(|e| e.tag != tag);
        list.push(Entry {
            tag,
            kind,
            count: data.len() as u32,
            value: Some(data),
            raw: [0; 4],
        });
    }

    fn ascii(text: &str) -> Vec<u8> {
        let mut v = text.as_bytes().to_vec();
        v.push(0);
        v
    }

    /// Append a directory to `out`, values first; returns its offset.
    fn append(f: &File, out: &mut Vec<u8>, mut list: Vec<Entry>, next: u32) -> Result<u32> {
        list.sort_by_key(|e| e.tag);
        let mut fixed = Vec::with_capacity(list.len());
        for e in list {
            let raw = match &e.value {
                Some(v) if v.len() > 4 => {
                    if out.len() % 2 == 1 {
                        out.push(0);
                    }
                    let at = offset(out)?;
                    out.extend_from_slice(v);
                    f.put32(at)
                }
                Some(v) => {
                    let mut r = [0u8; 4];
                    r[..v.len()].copy_from_slice(v);
                    r
                }
                None => e.raw,
            };
            fixed.push((e.tag, e.kind, e.count, raw));
        }
        if out.len() % 2 == 1 {
            out.push(0);
        }
        let at = offset(out)?;
        out.extend_from_slice(&f.put16(fixed.len() as u16));
        for (tag, kind, count, raw) in fixed {
            out.extend_from_slice(&f.put16(tag));
            out.extend_from_slice(&f.put16(kind));
            out.extend_from_slice(&f.put32(count));
            out.extend_from_slice(&raw);
        }
        out.extend_from_slice(&f.put32(next));
        Ok(at)
    }

    fn offset(out: &[u8]) -> Result<u32> {
        u32::try_from(out.len()).map_err(|_| {
            err(
                ErrorCode::LimitExceeded,
                "the TIFF file is too large to tag",
            )
        })
    }

    pub(super) fn set(bytes: &[u8], fields: &Fields) -> Result<Vec<u8>> {
        let bad = || err(ErrorCode::InputCorrupt, "the TIFF structure is damaged");
        let big = match bytes.get(..4) {
            Some(b"MM\0\x2A") => true,
            Some(b"II\x2A\0") => false,
            _ => {
                return Err(err(
                    ErrorCode::InputUnsupported,
                    "only classic TIFF can be tagged, not BigTIFF",
                ));
            }
        };
        let f = File { bytes, big };
        let first = f.u32(4).ok_or_else(bad)? as usize;
        let (mut main, next) = f.directory(first).ok_or_else(bad)?;
        for (tag, value) in [
            (0x010E, &fields.title),
            (0x013B, &fields.author),
            (0x8298, &fields.copyright),
        ] {
            if let Some(v) = value {
                set_entry(&mut main, tag, 2, ascii(v));
            }
        }
        let mut out = bytes.to_vec();
        if fields.date.is_some() || fields.comment.is_some() {
            let old = main.iter().find(|e| e.tag == 0x8769).map(|e| {
                if big {
                    u32::from_be_bytes(e.raw)
                } else {
                    u32::from_le_bytes(e.raw)
                }
            });
            let mut exif = match old {
                Some(at) => f.directory(at as usize).ok_or_else(bad)?.0,
                None => Vec::new(),
            };
            if let Some(d) = &fields.date {
                set_entry(&mut exif, 0x9003, 2, ascii(d));
            }
            if let Some(c) = &fields.comment {
                let mut raw = b"ASCII\0\0\0".to_vec();
                raw.extend_from_slice(c.as_bytes());
                set_entry(&mut exif, 0x9286, 7, raw);
            }
            let at = append(&f, &mut out, exif, 0)?;
            main.retain(|e| e.tag != 0x8769);
            main.push(Entry {
                tag: 0x8769,
                kind: 4,
                count: 1,
                value: None,
                raw: f.put32(at),
            });
        }
        let at = append(&f, &mut out, main, next)?;
        out[4..8].copy_from_slice(&f.put32(at));
        Ok(out)
    }
}
