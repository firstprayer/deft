//! File formats, extension mapping and content sniffing.

use std::path::Path;

macro_rules! formats {
    ($($v:ident => $ext:literal $(| $alt:literal)* , $mime:literal, $label:literal;)*) => {
        /// Every file format Deft can recognise.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Format { $($v,)* }

        impl Format {
            pub const ALL: &'static [Format] = &[$(Format::$v,)*];

            /// Canonical file extension, without the dot.
            pub fn ext(self) -> &'static str { match self { $(Format::$v => $ext,)* } }

            /// IANA media type.
            pub fn mime(self) -> &'static str { match self { $(Format::$v => $mime,)* } }

            /// Short human label used in help text, e.g. "pdf".
            pub fn label(self) -> &'static str { match self { $(Format::$v => $label,)* } }

            /// Parse a user-supplied extension or format name such as "jpg" or ".JPEG".
            pub fn from_name(name: &str) -> Option<Format> {
                let name = name.trim_start_matches('.').to_ascii_lowercase();
                match name.as_str() {
                    $($ext $(| $alt)* => Some(Format::$v),)*
                    _ => None,
                }
            }
        }
    };
}

formats! {
    Pdf => "pdf", "application/pdf", "pdf";
    Png => "png", "image/png", "png";
    Jpeg => "jpg" | "jpeg" | "jpe" | "jfif", "image/jpeg", "jpg";
    Webp => "webp", "image/webp", "webp";
    Gif => "gif", "image/gif", "gif";
    Tiff => "tiff" | "tif", "image/tiff", "tiff";
    Bmp => "bmp", "image/bmp", "bmp";
    Ico => "ico", "image/x-icon", "ico";
    Avif => "avif", "image/avif", "avif";
    Heic => "heic" | "heif", "image/heic", "heic";
    Svg => "svg", "image/svg+xml", "svg";
    Mp4 => "mp4" | "m4v", "video/mp4", "mp4";
    Webm => "webm", "video/webm", "webm";
    Mov => "mov", "video/quicktime", "mov";
    Mkv => "mkv", "video/x-matroska", "mkv";
    Avi => "avi", "video/x-msvideo", "avi";
    Ogv => "ogv", "video/ogg", "ogv";
    Mp3 => "mp3", "audio/mpeg", "mp3";
    Wav => "wav", "audio/wav", "wav";
    M4a => "m4a" | "aac", "audio/mp4", "m4a";
    Flac => "flac", "audio/flac", "flac";
    Ogg => "ogg" | "oga", "audio/ogg", "ogg";
    Opus => "opus", "audio/opus", "opus";
    Docx => "docx", "application/vnd.openxmlformats-officedocument.wordprocessingml.document", "docx";
    Odt => "odt", "application/vnd.oasis.opendocument.text", "odt";
    Rtf => "rtf", "application/rtf", "rtf";
    Xlsx => "xlsx" | "xlsm", "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet", "xlsx";
    Xls => "xls", "application/vnd.ms-excel", "xls";
    Ods => "ods", "application/vnd.oasis.opendocument.spreadsheet", "ods";
    Csv => "csv" | "tsv", "text/csv", "csv";
    Json => "json", "application/json", "json";
    Jsonl => "jsonl" | "ndjson", "application/x-ndjson", "jsonl";
    Yaml => "yaml" | "yml", "application/yaml", "yaml";
    Toml => "toml", "application/toml", "toml";
    Xml => "xml", "application/xml", "xml";
    Pptx => "pptx", "application/vnd.openxmlformats-officedocument.presentationml.presentation", "pptx";
    Odp => "odp", "application/vnd.oasis.opendocument.presentation", "odp";
    Epub => "epub", "application/epub+zip", "epub";
    Eml => "eml", "message/rfc822", "eml";
    Vcf => "vcf" | "vcard", "text/vcard", "vcf";
    Ics => "ics" | "ical", "text/calendar", "ics";
    Gpx => "gpx", "application/gpx+xml", "gpx";
    Kml => "kml", "application/vnd.google-earth.kml+xml", "kml";
    Sqlite => "sqlite" | "db" | "sqlite3", "application/vnd.sqlite3", "sqlite";
    Md => "md" | "markdown" | "mdown", "text/markdown", "md";
    Html => "html" | "htm" | "xhtml", "text/html", "html";
    Txt => "txt" | "text", "text/plain", "txt";
    Srt => "srt", "application/x-subrip", "srt";
    Vtt => "vtt", "text/vtt", "vtt";
    Ass => "ass" | "ssa", "text/x-ssa", "ass";
    Zip => "zip", "application/zip", "zip";
    Tar => "tar", "application/x-tar", "tar";
    TarGz => "tar.gz" | "tgz", "application/gzip", "tar.gz";
    Gz => "gz", "application/gzip", "gz";
    TarBz2 => "tar.bz2" | "tbz2", "application/x-bzip2", "tar.bz2";
    TarXz => "tar.xz" | "txz", "application/x-xz", "tar.xz";
    TarZst => "tar.zst" | "tzst", "application/zstd", "tar.zst";
    SevenZ => "7z", "application/x-7z-compressed", "7z";
    Rar => "rar", "application/vnd.rar", "rar";
}

/// What an OLE compound file (the container of old Office files) holds,
/// from the names of its streams: "encrypted" for an Office file saved with
/// a password, "word" or "powerpoint" for the old binary formats.
pub fn ole_content(bytes: &[u8]) -> Option<&'static str> {
    if !bytes.starts_with(&[0xD0, 0xCF, 0x11, 0xE0]) {
        return None;
    }
    let utf16 =
        |name: &str| -> Vec<u8> { name.encode_utf16().flat_map(u16::to_le_bytes).collect() };
    let has = |name: &str| {
        let needle = utf16(name);
        bytes.windows(needle.len()).any(|w| w == needle)
    };
    if has("EncryptedPackage") {
        Some("encrypted")
    } else if has("WordDocument") {
        Some("word")
    } else if has("PowerPoint Document") {
        Some("powerpoint")
    } else {
        None
    }
}

impl Format {
    /// Format implied by a path's extension. Handles double extensions like `.tar.gz`.
    pub fn from_path(path: &Path) -> Option<Format> {
        let name = path.file_name()?.to_str()?.to_ascii_lowercase();
        for (ends, format) in [
            (".tar.gz", Format::TarGz),
            (".tgz", Format::TarGz),
            (".tar.bz2", Format::TarBz2),
            (".tbz2", Format::TarBz2),
            (".tar.xz", Format::TarXz),
            (".txz", Format::TarXz),
            (".tar.zst", Format::TarZst),
            (".tzst", Format::TarZst),
        ] {
            if name.ends_with(ends) {
                return Some(format);
            }
        }
        Format::from_name(path.extension()?.to_str()?)
    }

    pub fn is_image(self) -> bool {
        use Format::*;
        matches!(
            self,
            Png | Jpeg | Webp | Gif | Tiff | Bmp | Ico | Avif | Heic | Svg
        )
    }
    pub fn is_video(self) -> bool {
        matches!(
            self,
            Format::Mp4 | Format::Webm | Format::Mov | Format::Mkv | Format::Avi | Format::Ogv
        )
    }
    pub fn is_audio(self) -> bool {
        use Format::*;
        matches!(self, Mp3 | Wav | M4a | Flac | Ogg | Opus)
    }
    pub fn is_text(self) -> bool {
        use Format::*;
        matches!(
            self,
            Md | Html
                | Txt
                | Csv
                | Json
                | Jsonl
                | Yaml
                | Toml
                | Xml
                | Srt
                | Vtt
                | Ass
                | Svg
                | Eml
                | Vcf
                | Ics
                | Gpx
                | Kml
        )
    }
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.label())
    }
}

/// Identify a file from its first bytes, falling back to the extension for
/// text formats (which have no reliable signature).
pub fn sniff(head: &[u8], path: Option<&Path>) -> Option<Format> {
    let by_ext = path.and_then(Format::from_path);
    let starts = |sig: &[u8]| head.starts_with(sig);
    let at = |offset: usize, sig: &[u8]| {
        head.len() >= offset + sig.len() && &head[offset..offset + sig.len()] == sig
    };

    if starts(b"%PDF-") || find(head, b"%PDF-").is_some_and(|i| i < 1024) {
        return Some(Format::Pdf);
    }
    if starts(b"\x89PNG\r\n\x1a\n") {
        return Some(Format::Png);
    }
    if starts(&[0xFF, 0xD8, 0xFF]) {
        return Some(Format::Jpeg);
    }
    if starts(b"GIF87a") || starts(b"GIF89a") {
        return Some(Format::Gif);
    }
    if starts(b"RIFF") && at(8, b"WEBP") {
        return Some(Format::Webp);
    }
    if starts(b"RIFF") && at(8, b"WAVE") {
        return Some(Format::Wav);
    }
    if starts(b"RIFF") && at(8, b"AVI ") {
        return Some(Format::Avi);
    }
    if starts(b"II*\0") || starts(b"MM\0*") {
        return Some(Format::Tiff);
    }
    if starts(b"BM") && head.len() > 14 && by_ext.is_none_or(|f| f == Format::Bmp) {
        return Some(Format::Bmp);
    }
    if at(4, b"ftyp") && head.len() >= 12 {
        let brand = &head[8..12];
        return Some(match brand {
            b"avif" | b"avis" => Format::Avif,
            b"heic" | b"heix" | b"hevc" | b"heim" | b"heis" | b"mif1" | b"msf1" => Format::Heic,
            b"qt  " => Format::Mov,
            b"M4A " | b"M4B " => Format::M4a,
            _ => match by_ext {
                Some(f @ (Format::M4a | Format::Mov)) => f,
                _ => Format::Mp4,
            },
        });
    }
    if starts(&[0, 0, 1, 0]) && by_ext.is_none_or(|f| f == Format::Ico) {
        return Some(Format::Ico);
    }
    if starts(&[0x1A, 0x45, 0xDF, 0xA3]) {
        return Some(if find(&head[..head.len().min(64)], b"webm").is_some() {
            Format::Webm
        } else {
            Format::Mkv
        });
    }
    if starts(b"ID3")
        || (head.len() > 1
            && head[0] == 0xFF
            && head[1] & 0xE0 == 0xE0
            // FF FE is the byte order mark of UTF-16 text.
            && head[1] != 0xFE
            && by_ext != Some(Format::M4a))
    {
        return Some(Format::Mp3);
    }
    if starts(b"fLaC") {
        return Some(Format::Flac);
    }
    if starts(b"OggS") {
        return Some(
            if find(&head[..head.len().min(64)], b"OpusHead").is_some() {
                Format::Opus
            } else if find(head, b"\x80theora").is_some() || by_ext == Some(Format::Ogv) {
                // Ogg holding Theora is a video, not a sound.
                Format::Ogv
            } else {
                Format::Ogg
            },
        );
    }
    if starts(b"PK\x03\x04") {
        // OOXML / ODF containers are zips; tell them apart by well-known entry names.
        if find(head, b"word/").is_some() {
            return Some(Format::Docx);
        }
        if find(head, b"xl/").is_some() {
            return Some(Format::Xlsx);
        }
        if find(head, b"ppt/").is_some() {
            return Some(Format::Pptx);
        }
        if find(head, b"application/vnd.oasis.opendocument.text").is_some() {
            return Some(Format::Odt);
        }
        if find(head, b"application/vnd.oasis.opendocument.spreadsheet").is_some() {
            return Some(Format::Ods);
        }
        if find(head, b"application/vnd.oasis.opendocument.presentation").is_some() {
            return Some(Format::Odp);
        }
        if find(head, b"application/epub+zip").is_some() {
            return Some(Format::Epub);
        }
        return Some(match by_ext {
            Some(
                f @ (Format::Docx
                | Format::Xlsx
                | Format::Pptx
                | Format::Odt
                | Format::Ods
                | Format::Odp
                | Format::Epub),
            ) => f,
            _ => Format::Zip,
        });
    }
    if starts(&[0xD0, 0xCF, 0x11, 0xE0]) {
        return Some(Format::Xls);
    }
    if starts(&[0x1F, 0x8B]) {
        return Some(match by_ext {
            Some(Format::TarGz) => Format::TarGz,
            _ if path.is_some_and(|p| {
                p.to_string_lossy()
                    .to_ascii_lowercase()
                    .ends_with(".tar.gz")
            }) =>
            {
                Format::TarGz
            }
            _ => by_ext.filter(|f| *f == Format::TarGz).unwrap_or(Format::Gz),
        });
    }
    if at(257, b"ustar") {
        return Some(Format::Tar);
    }
    if starts(b"Rar!\x1a\x07") {
        return Some(Format::Rar);
    }
    if starts(b"SQLite format 3\0") {
        return Some(Format::Sqlite);
    }
    if starts(b"7z\xBC\xAF\x27\x1C") {
        return Some(Format::SevenZ);
    }
    // Compressed tarballs have no signature of their own beyond the compressor's.
    if starts(b"BZh")
        || starts(&[0xFD, b'7', b'z', b'X', b'Z', 0])
        || starts(&[0x28, 0xB5, 0x2F, 0xFD])
    {
        return by_ext;
    }
    if starts(b"{\\rtf") {
        return Some(Format::Rtf);
    }
    // Text formats: trust the extension, then look at the content.
    if let Some(f) = by_ext.filter(|f| f.is_text()) {
        return Some(f);
    }
    let text = std::str::from_utf8(&head[..head.len().min(512)])
        .ok()
        .map(str::trim_start);
    if let Some(t) = text {
        let lower = t.to_ascii_lowercase();
        if lower.starts_with("<svg") || (lower.starts_with("<?xml") && lower.contains("<svg")) {
            return Some(Format::Svg);
        }
        if lower.starts_with("<!doctype html") || lower.starts_with("<html") {
            return Some(Format::Html);
        }
        if t.starts_with("WEBVTT") {
            return Some(Format::Vtt);
        }
        if lower.starts_with("<?xml") {
            return Some(Format::Xml);
        }
    }
    by_ext
}

fn find(hay: &[u8], needle: &[u8]) -> Option<usize> {
    hay.windows(needle.len()).position(|w| w == needle)
}

/// Read enough of a file to sniff it.
pub fn sniff_file(path: &Path) -> std::io::Result<Option<Format>> {
    use std::io::Read;
    let mut head = vec![0u8; 4096];
    let mut file = std::fs::File::open(path)?;
    let mut filled = 0;
    while filled < head.len() {
        let n = file.read(&mut head[filled..])?;
        if n == 0 {
            break;
        }
        filled += n;
    }
    head.truncate(filled);
    Ok(sniff(&head, Some(path)))
}

#[cfg(test)]
#[path = "format_tests.rs"]
mod tests;
