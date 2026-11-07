//! Unicode maps for fonts the text reader cannot read on its own. They are
//! added to the copy of a document that is rendered and read, never to output.

use crate::deref;
use hayro::hayro_interpret::hayro_cmap::{BfString, CMap, CMapName, load_embedded};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};

/// The first of 128 private-use code points that carry the high bytes of a
/// GBK-encoded simple font, for the text reader to join in pairs.
pub(crate) const GBK_BYTES: u32 = 0xF780;

/// Add or repair /ToUnicode maps so the text reader reads these fonts.
pub(crate) fn add_text_maps(doc: &mut Document) {
    let fonts: Vec<ObjectId> = doc
        .objects
        .iter()
        .filter(|(_, o)| o.as_dict().is_ok_and(|d| d.has_type(b"Font")))
        .map(|(id, _)| *id)
        .collect();
    for id in fonts {
        let Ok(font) = doc.get_dictionary(id) else {
            continue;
        };
        let map = match font.get(b"ToUnicode").and_then(Object::as_reference) {
            Ok(map) => {
                repair_empty_entries(doc, map);
                continue;
            }
            Err(_) if font.has(b"ToUnicode") => continue,
            Err(_) => match font.get(b"Subtype").and_then(Object::as_name) {
                Ok(b"Type0") => collection_map(doc, font),
                Ok(_) => gbk_map(doc, font).or_else(|| numbered_names_map(doc, font)),
                Err(_) => None,
            },
        };
        if let Some(text) = map {
            let stream = doc.add_object(Stream::new(Dictionary::new(), text.into_bytes()));
            if let Ok(font) = doc.get_dictionary_mut(id) {
                font.set("ToUnicode", stream);
            }
        }
    }
}

/// A CMap with these code-to-text entries, the text as UTF-16BE hex.
fn cmap(width: usize, entries: &[(u32, String)]) -> String {
    let mut out = String::from(
        "/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n\
         /CMapName /Deft-UCS def /CMapType 2 def\n",
    );
    let hex = |s: &str| -> String { s.encode_utf16().map(|u| format!("{u:04X}")).collect() };
    let code = |c: u32| format!("{c:0width$X}", width = width * 2);
    out.push_str(&format!(
        "1 begincodespacerange <{}> <{}> endcodespacerange\n",
        code(0),
        code(if width == 1 { 0xFF } else { 0xFFFF })
    ));
    for chunk in entries.chunks(100) {
        out.push_str(&format!("{} beginbfchar\n", chunk.len()));
        for (c, text) in chunk {
            out.push_str(&format!("<{}> <{}>\n", code(*c), hex(text)));
        }
        out.push_str("endbfchar\n");
    }
    out.push_str("endcmap CMapName currentdict /CMap defineresource pop end end\n");
    out
}

/// An empty destination (`<03f2> <>`) is a valid mapping to no text, which
/// HarfBuzz-based writers give the parts of a shaped Arabic cluster. The
/// reader stops at it and loses the rest of the map, so it becomes a
/// zero-width space, which the text reader drops.
fn repair_empty_entries(doc: &mut Document, map: ObjectId) {
    let Ok(stream) = doc.get_object(map).and_then(Object::as_stream) else {
        return;
    };
    let Ok(text) = stream.get_plain_content() else {
        return;
    };
    if !text.windows(2).any(|w| w == b"<>") {
        return;
    }
    let mut fixed = Vec::with_capacity(text.len() + 32);
    let mut rest = text.as_slice();
    while let Some(at) = rest.windows(2).position(|w| w == b"<>") {
        fixed.extend_from_slice(&rest[..at]);
        fixed.extend_from_slice(b"<200B>");
        rest = &rest[at + 2..];
    }
    fixed.extend_from_slice(rest);
    doc.objects
        .insert(map, Object::Stream(Stream::new(Dictionary::new(), fixed)));
}

/// A font that is not embedded.
fn is_embedded(doc: &Document, font: &Dictionary) -> bool {
    let descriptor = |d: &Dictionary| {
        d.get(b"FontDescriptor")
            .ok()
            .map(|o| deref(doc, o))
            .and_then(|o| o.as_dict().ok())
            .is_some_and(|fd| fd.has(b"FontFile") || fd.has(b"FontFile2") || fd.has(b"FontFile3"))
    };
    descriptor(font)
}

/// Old Chinese writers name a system font in GBK (宋体, 仿宋_GB2312), give it
/// /WinAnsiEncoding and write GBK bytes, which viewers pass to that font.
/// Bytes from 0x80 come out as private-use characters for the reader to join.
fn gbk_map(doc: &Document, font: &Dictionary) -> Option<String> {
    let name = font.get(b"BaseFont").and_then(Object::as_name).ok()?;
    if is_embedded(doc, font)
        || font.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Type3")
    {
        return None;
    }
    let upper = String::from_utf8_lossy(name).to_ascii_uppercase();
    let named = upper.contains("GB2312") || upper.contains("GBK");
    let non_ascii = name.iter().any(|b| *b >= 0x80);
    let decodes = non_ascii && {
        let (text, _, bad) = encoding_rs::GBK.decode(name);
        !bad && text.chars().any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c))
    };
    if !named && !decodes {
        return None;
    }
    let entries: Vec<(u32, String)> = (0x20..0x80u32)
        .map(|b| (b, char::from(b as u8).to_string()))
        .chain(
            (0x80..0x100u32)
                .filter_map(|b| Some((b, char::from_u32(GBK_BYTES + b - 0x80)?.to_string()))),
        )
        .collect();
    Some(cmap(1, &entries))
}

/// Type3 and TeX fonts often name their glyphs after the character code:
/// /g65 or /a65 for code 65. Such names are not in the glyph list, so the
/// code is read as the character it is in the standard encoding, as
/// Ghostscript reads it.
fn numbered_names_map(doc: &Document, font: &Dictionary) -> Option<String> {
    let base = font
        .get(b"BaseFont")
        .and_then(Object::as_name)
        .unwrap_or(b"");
    if String::from_utf8_lossy(base).contains("Dingbat") {
        return None;
    }
    let encoding = deref(doc, font.get(b"Encoding").ok()?).as_dict().ok()?;
    let differences = deref(doc, encoding.get(b"Differences").ok()?)
        .as_array()
        .ok()?;
    let mut entries = Vec::new();
    let mut code = 0u32;
    for item in differences {
        match deref(doc, item) {
            Object::Integer(n) => code = *n as u32,
            Object::Name(name) => {
                let digits = name.iter().position(u8::is_ascii_digit);
                let numbered = digits.is_some_and(|at| {
                    (1..=2).contains(&at)
                        && name[..at].iter().all(u8::is_ascii_alphabetic)
                        && std::str::from_utf8(&name[at..])
                            .ok()
                            .and_then(|d| d.parse::<u32>().ok())
                            == Some(code)
                });
                if numbered && let Some(c) = standard_char(code) {
                    entries.push((code, c.to_string()));
                }
                code += 1;
            }
            _ => {}
        }
    }
    (entries.len() >= 4).then(|| cmap(1, &entries))
}

/// A code as Windows-1252 text, for printable codes.
fn standard_char(code: u32) -> Option<char> {
    let byte = u8::try_from(code).ok()?;
    let bytes = [byte];
    let (text, _, bad) = encoding_rs::WINDOWS_1252.decode(&bytes);
    let c = text.chars().next()?;
    (!bad && !c.is_control()).then_some(c)
}

/// A CID font in a standard collection (Adobe-Japan1, GB1, CNS1, Korea1)
/// with an Identity encoding and no /ToUnicode: its codes are CIDs, and the
/// collection's own table names their characters.
fn collection_map(doc: &Document, font: &Dictionary) -> Option<String> {
    let encoding = font.get(b"Encoding").and_then(Object::as_name).ok()?;
    if encoding != b"Identity-H" && encoding != b"Identity-V" {
        return None;
    }
    let descendant = deref(doc, font.get(b"DescendantFonts").ok()?)
        .as_array()
        .ok()?
        .first()
        .map(|o| deref(doc, o))?
        .as_dict()
        .ok()?;
    let info = deref(doc, descendant.get(b"CIDSystemInfo").ok()?)
        .as_dict()
        .ok()?;
    let ordering = deref(doc, info.get(b"Ordering").ok()?).as_str().ok()?;
    let (table, last) = match ordering {
        b"Japan1" => (CMapName::AdobeJapan1Ucs2, 23_059),
        b"GB1" => (CMapName::AdobeGb1Ucs2, 30_283),
        b"CNS1" => (CMapName::AdobeCns1Ucs2, 19_178),
        b"Korea1" => (CMapName::AdobeKorea1Ucs2, 18_351),
        _ => return None,
    };
    let ucs2 = CMap::parse(load_embedded(table)?, load_embedded)?;
    let entries: Vec<(u32, String)> = (1..=last)
        .filter_map(|cid| {
            let text = match ucs2.lookup_bf_string(cid)? {
                BfString::Char(c) => c.to_string(),
                BfString::String(s) => s,
            };
            Some((cid, text))
        })
        .collect();
    Some(cmap(2, &entries))
}

// ------------------------------------------------------------------ fallback

use hayro::hayro_interpret::InterpreterSettings;
use hayro::hayro_interpret::font::{FallbackFontQuery, FontData, FontQuery};
use hayro::hayro_interpret::hayro_cmap::CidFamily;
use std::sync::{Arc, OnceLock};

/// Fonts that hold Chinese, Japanese or Korean, where the systems keep them.
/// A PDF may name such a font without embedding it, as Japanese and Chinese
/// writers often do; deft carries no font that large, so it borrows one.
fn cjk_candidates(family: &CidFamily) -> &'static [&'static str] {
    match family {
        CidFamily::AdobeJapan1 => &[
            "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
            "C:\\Windows\\Fonts\\msgothic.ttc",
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        ],
        CidFamily::AdobeKorea1 => &[
            "/System/Library/Fonts/AppleSDGothicNeo.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
            "C:\\Windows\\Fonts\\malgun.ttf",
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        ],
        _ => &[
            "/System/Library/Fonts/Hiragino Sans GB.ttc",
            "/System/Library/Fonts/STHeiti Light.ttc",
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/google-noto-cjk/NotoSansCJK-Regular.ttc",
            "/usr/share/fonts/truetype/droid/DroidSansFallbackFull.ttf",
            "C:\\Windows\\Fonts\\msyh.ttc",
            "C:\\Windows\\Fonts\\simsun.ttc",
            "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        ],
    }
}

/// The first CJK font found for a character collection, read once.
fn cjk_font(family: &CidFamily) -> Option<FontData> {
    static FONTS: OnceLock<[Option<FontData>; 3]> = OnceLock::new();
    let load = |f: CidFamily| -> Option<FontData> {
        cjk_candidates(&f).iter().find_map(|path| {
            let bytes = std::fs::read(path).ok()?;
            Some(Arc::new(bytes) as FontData)
        })
    };
    let fonts = FONTS.get_or_init(|| {
        [
            load(CidFamily::AdobeJapan1),
            load(CidFamily::AdobeKorea1),
            load(CidFamily::AdobeGB1),
        ]
    });
    match family {
        CidFamily::AdobeJapan1 => fonts[0].clone(),
        CidFamily::AdobeKorea1 => fonts[1].clone(),
        CidFamily::AdobeGB1 | CidFamily::AdobeCNS1 => fonts[2].clone(),
        _ => None,
    }
}

fn cjk_family(query: &FallbackFontQuery) -> Option<&CidFamily> {
    let family = &query.character_collection.as_ref()?.family;
    matches!(
        family,
        CidFamily::AdobeJapan1
            | CidFamily::AdobeGB1
            | CidFamily::AdobeCNS1
            | CidFamily::AdobeKorea1
    )
    .then_some(family)
}

/// Settings for reading and drawing pages: the built-in standard fonts, and
/// a system CJK font for Chinese, Japanese and Korean fonts that are not
/// embedded.
pub(crate) fn interpreter_settings() -> InterpreterSettings {
    InterpreterSettings {
        font_resolver: Arc::new(|query| match query {
            FontQuery::Standard(s) => Some(s.get_font_data()),
            FontQuery::Fallback(f) => match cjk_family(f).and_then(cjk_font) {
                Some(data) => Some((data, 0)),
                None => Some(f.pick_standard_font().get_font_data()),
            },
        }),
        ..InterpreterSettings::default()
    }
}

/// Whether the document uses a Chinese, Japanese or Korean font that is not
/// embedded and no system font can stand in for.
pub fn missing_cjk_font(doc: &Document) -> bool {
    doc.objects.values().any(|o| {
        let Ok(font) = o.as_dict() else {
            return false;
        };
        if !font.has_type(b"Font")
            || font.get(b"Subtype").and_then(Object::as_name).ok() != Some(b"Type0")
        {
            return false;
        }
        let Some(descendant) = font
            .get(b"DescendantFonts")
            .ok()
            .map(|o| deref(doc, o))
            .and_then(|o| o.as_array().ok())
            .and_then(|a| a.first())
            .map(|o| deref(doc, o))
            .and_then(|o| o.as_dict().ok())
        else {
            return false;
        };
        if is_embedded(doc, descendant) {
            return false;
        }
        let ordering = descendant
            .get(b"CIDSystemInfo")
            .ok()
            .map(|o| deref(doc, o))
            .and_then(|o| o.as_dict().ok())
            .and_then(|i| i.get(b"Ordering").ok())
            .and_then(|o| deref(doc, o).as_str().ok())
            .unwrap_or(b"");
        let family = CidFamily::from_registry_ordering(b"Adobe", ordering);
        matches!(
            family,
            CidFamily::AdobeJapan1
                | CidFamily::AdobeGB1
                | CidFamily::AdobeCNS1
                | CidFamily::AdobeKorea1
        ) && cjk_font(&family).is_none()
    })
}

#[cfg(test)]
#[path = "fonts_tests.rs"]
mod tests;
