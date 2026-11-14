//! Text encodings and base64, for commands that read text of unknown origin,
//! and what a media or JSON file holds, for `deft info`.
pub mod media;
pub mod web;

pub use encoding_rs::Encoding;

pub fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// UTF-8 apart from a few stray bytes. Legacy encodings form a valid sequence
/// only now and then, so their invalid bytes far outnumber the valid ones.
fn mostly_utf8(bytes: &[u8]) -> bool {
    let (mut valid, mut invalid) = (0, 0);
    for chunk in bytes.utf8_chunks() {
        valid += chunk.valid().bytes().filter(|b| !b.is_ascii()).count();
        invalid += chunk.invalid().len();
    }
    invalid == 0 || valid >= 2 * invalid
}

/// Latin text in UTF-16 has a zero in every other byte.
fn utf16_without_bom(bytes: &[u8]) -> Option<&'static encoding_rs::Encoding> {
    let zeros = |start: usize| {
        bytes
            .iter()
            .skip(start)
            .step_by(2)
            .filter(|b| **b == 0)
            .count()
    };
    let half = bytes.len() / 2;
    let most = |n: usize| half > 0 && n * 10 >= half * 9;
    match (most(zeros(0)), most(zeros(1))) {
        (false, true) => Some(encoding_rs::UTF_16LE),
        (true, false) => Some(encoding_rs::UTF_16BE),
        _ => None,
    }
}

/// True for bytes that are no text at all: NUL bytes in anything but UTF-16.
pub fn looks_binary(bytes: &[u8]) -> bool {
    encoding_rs::Encoding::for_bom(bytes).is_none()
        && utf16_without_bom(bytes).is_none()
        && bytes.contains(&0)
}

/// The text encoding of some bytes: a byte order mark, valid UTF-8, or a guess.
pub fn detect_encoding(bytes: &[u8]) -> &'static encoding_rs::Encoding {
    if let Some((enc, _)) = encoding_rs::Encoding::for_bom(bytes) {
        return enc;
    }
    if let Some(utf16) = utf16_without_bom(bytes) {
        return utf16;
    }
    if mostly_utf8(bytes) {
        return encoding_rs::UTF_8;
    }
    let mut d = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Deny);
    d.feed(bytes, true);
    d.guess(None, chardetng::Utf8Detection::Allow)
}

pub fn find_encoding(name: &str) -> Option<&'static encoding_rs::Encoding> {
    let name = name.trim();
    let alias = match name.to_ascii_lowercase().as_str() {
        "latin1" | "latin-1" | "iso8859-1" => "iso-8859-1",
        "utf8" => "utf-8",
        "utf16" | "utf-16" => "utf-16le",
        // The web standard calls these windows-1252, which is not ASCII.
        "ascii" | "us-ascii" => return None,
        "cp1252" => "windows-1252",
        "sjis" | "shiftjis" => "shift_jis",
        "gb2312" | "gb18030" => "gb18030",
        _ => name,
    }
    .to_string();
    encoding_rs::Encoding::for_label(alias.as_bytes())
}

/// Decoded text and whether any byte was invalid in the source encoding.
pub fn decode_text(bytes: &[u8], from: &'static encoding_rs::Encoding) -> (String, bool) {
    let (text, _, bad) = from.decode(bytes);
    (text.into_owned(), bad)
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
