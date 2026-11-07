//! File-level capabilities that do not depend on a file's type: hashing,
//! duplicate detection, batch renames, base64 and text encodings.

use deft_core::error::{ErrorCode, Result, err};
pub use encoding_rs::Encoding;
use regex::Regex;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Algorithm {
    Md5,
    Sha1,
    Sha256,
    Sha512,
    Blake3,
    Crc32,
}

impl Algorithm {
    pub fn parse(s: &str) -> Algorithm {
        match s {
            "md5" => Algorithm::Md5,
            "sha1" => Algorithm::Sha1,
            "sha512" => Algorithm::Sha512,
            "blake3" => Algorithm::Blake3,
            "crc32" => Algorithm::Crc32,
            _ => Algorithm::Sha256,
        }
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Feed a file through `update` in blocks.
fn stream(path: &Path, mut update: impl FnMut(&[u8])) -> Result<()> {
    let mut file = std::fs::File::open(path)
        .map_err(|e| err(ErrorCode::InputNotFound, format!("{}: {e}", path.display())))?;
    let mut buf = vec![0u8; 1 << 20];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(());
        }
        update(&buf[..n]);
    }
}

fn digest<D: sha2::Digest>(path: &Path) -> Result<String> {
    let mut d = D::new();
    stream(path, |b| d.update(b))?;
    Ok(hex(&d.finalize()))
}

/// Lower-case hex digest of a file.
pub fn hash_file(path: &Path, algorithm: Algorithm) -> Result<String> {
    match algorithm {
        Algorithm::Md5 => digest::<md5::Md5>(path),
        Algorithm::Sha1 => digest::<sha1::Sha1>(path),
        Algorithm::Sha256 => digest::<sha2::Sha256>(path),
        Algorithm::Sha512 => digest::<sha2::Sha512>(path),
        Algorithm::Blake3 => {
            let mut h = blake3::Hasher::new();
            stream(path, |b| {
                h.update(b);
            })?;
            Ok(h.finalize().to_hex().to_string())
        }
        Algorithm::Crc32 => {
            let mut h = crc32fast::Hasher::new();
            stream(path, |b| h.update(b))?;
            Ok(format!("{:08x}", h.finalize()))
        }
    }
}

/// Every file under the given paths, in sorted order, skipping hidden entries
/// found while walking directories.
pub fn walk(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    fn visit(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with('.'))
            {
                continue;
            }
            if entry.file_type()?.is_symlink() {
                continue;
            }
            if path.is_dir() {
                visit(&path, out)?;
            } else {
                out.push(path);
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    for p in paths {
        if p.is_dir() {
            let mut found = Vec::new();
            visit(p, &mut found)?;
            found.sort();
            out.extend(found);
        } else {
            out.push(p.clone());
        }
    }
    Ok(out)
}

/// Files with identical content.
#[derive(Clone, Debug, PartialEq)]
pub struct DuplicateGroup {
    pub size: u64,
    pub files: Vec<PathBuf>,
}

/// False for a further hard link to a file already seen: it takes no extra
/// space, so it is not a copy.
#[cfg(unix)]
fn first_link(meta: &std::fs::Metadata, seen: &mut HashSet<(u64, u64)>) -> bool {
    use std::os::unix::fs::MetadataExt;
    seen.insert((meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
fn first_link(_: &std::fs::Metadata, _: &mut HashSet<(u64, u64)>) -> bool {
    true
}

/// Find identical files. Sizes first, so only possible matches are read.
pub fn duplicates(files: &[PathBuf]) -> Result<Vec<DuplicateGroup>> {
    let mut by_size: BTreeMap<u64, Vec<&PathBuf>> = BTreeMap::new();
    let mut seen = HashSet::new();
    for f in files {
        let meta = std::fs::metadata(f)?;
        if meta.len() > 0 && first_link(&meta, &mut seen) {
            by_size.entry(meta.len()).or_default().push(f);
        }
    }
    let mut out = Vec::new();
    for (size, same) in by_size.into_iter().rev() {
        if same.len() < 2 {
            continue;
        }
        let mut by_hash: HashMap<String, Vec<PathBuf>> = HashMap::new();
        for f in same {
            by_hash
                .entry(hash_file(f, Algorithm::Blake3)?)
                .or_default()
                .push(f.clone());
        }
        let mut groups: Vec<Vec<PathBuf>> = by_hash.into_values().filter(|g| g.len() > 1).collect();
        groups.sort();
        out.extend(
            groups
                .into_iter()
                .map(|files| DuplicateGroup { size, files }),
        );
    }
    Ok(out)
}

/// How to compute new names. Every part is optional and they apply in order:
/// the regex on the file stem, the case change, then the template.
#[derive(Clone, Debug, Default)]
pub struct RenameRule {
    pub find: Option<Regex>,
    pub replace: String,
    /// `upper` or `lower`.
    pub case: Option<String>,
    /// Template with `{stem}`, `{ext}`, `{n}`, `{n:03}` and `{parent}`.
    pub name: Option<String>,
    pub start: usize,
}

fn expand_template(template: &str, stem: &str, ext: &str, parent: &str, n: usize) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            out.push_str(&rest[open..]);
            return out;
        };
        let field = &after[..close];
        match field {
            "stem" => out.push_str(stem),
            "ext" => out.push_str(ext),
            "parent" => out.push_str(parent),
            "n" => out.push_str(&n.to_string()),
            f if f.starts_with("n:") => {
                let width: usize = f[2..].trim_start_matches('0').parse().unwrap_or(0);
                out.push_str(&format!("{n:0width$}"));
            }
            other => {
                out.push('{');
                out.push_str(other);
                out.push('}');
            }
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

/// Work out each file's new path, refusing collisions before anything moves.
pub fn rename_plan(files: &[PathBuf], rule: &RenameRule) -> Result<Vec<(PathBuf, PathBuf)>> {
    let mut plan = Vec::new();
    for (i, file) in files.iter().enumerate() {
        let stem_os = file.file_stem().unwrap_or_default().to_string_lossy();
        let ext = file
            .extension()
            .map(|e| e.to_string_lossy().to_string())
            .unwrap_or_default();
        let dir = file.parent().unwrap_or(Path::new(""));
        let parent = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let mut stem = stem_os.to_string();
        if let Some(re) = &rule.find {
            stem = re.replace_all(&stem, rule.replace.as_str()).to_string();
        }
        match rule.case.as_deref() {
            Some("upper") => stem = stem.to_uppercase(),
            Some("lower") => stem = stem.to_lowercase(),
            _ => {}
        }
        let name = match &rule.name {
            Some(t) => expand_template(t, &stem, &ext, &parent, rule.start + i),
            None if ext.is_empty() => stem,
            None => format!("{stem}.{ext}"),
        };
        if name.is_empty() || name.contains('/') || name.contains('\\') {
            return Err(err(
                ErrorCode::BadValue,
                format!("{}: {name:?} is not a file name", file.display()),
            ));
        }
        plan.push((file.clone(), dir.join(name)));
    }
    let sources: HashSet<&PathBuf> = plan.iter().map(|(a, _)| a).collect();
    let mut targets = HashSet::new();
    for (from, to) in &plan {
        if !targets.insert(to) {
            return Err(err(
                ErrorCode::OutputExists,
                format!("two files would become {}", to.display()),
            ));
        }
        // On a case-insensitive disk, a.txt -> A.txt finds itself.
        let same_file = match (from.canonicalize(), to.canonicalize()) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        };
        if from != to && to.exists() && !same_file && !sources.contains(to) {
            return Err(err(
                ErrorCode::OutputExists,
                format!("{} already exists", to.display()),
            ));
        }
    }
    plan.retain(|(a, b)| a != b);
    Ok(plan)
}

/// Move files in two steps, so chains and swaps such as a->b, b->a are safe.
pub fn apply_renames(plan: &[(PathBuf, PathBuf)]) -> Result<()> {
    let staged: Vec<PathBuf> = plan
        .iter()
        .enumerate()
        .map(|(i, (from, _))| from.with_file_name(format!(".deft-rename-{i}")))
        .collect();
    for ((from, _), tmp) in plan.iter().zip(&staged) {
        std::fs::rename(from, tmp)?;
    }
    for ((_, to), tmp) in plan.iter().zip(&staged) {
        std::fs::rename(tmp, to)?;
    }
    Ok(())
}

pub fn base64_encode(bytes: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

/// Decode base64: accepts line breaks, the URL-safe alphabet, missing padding
/// and a `data:...;base64,` prefix.
pub fn base64_decode(text: &str) -> std::result::Result<Vec<u8>, String> {
    use base64::Engine;
    use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
    let text = text.trim();
    let body = text
        .strip_prefix("data:")
        .and_then(|t| t.split_once(','))
        .map_or(text, |(_, b)| b);
    let clean: String = body
        .chars()
        .filter(|c| !c.is_whitespace())
        .map(|c| match c {
            '-' => '+',
            '_' => '/',
            c => c,
        })
        .collect();
    let config =
        GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent);
    GeneralPurpose::new(&base64::alphabet::STANDARD, config)
        .decode(clean)
        .map_err(|e| e.to_string())
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

/// Encode text. Only UTF-8 and UTF-16 can hold everything; others report
/// whether characters had to be replaced.
pub fn encode_text(text: &str, to: &'static encoding_rs::Encoding, bom: bool) -> (Vec<u8>, bool) {
    if to == encoding_rs::UTF_16LE || to == encoding_rs::UTF_16BE {
        let little = to == encoding_rs::UTF_16LE;
        let mut out = Vec::new();
        if bom {
            out.extend_from_slice(if little { &[0xFF, 0xFE] } else { &[0xFE, 0xFF] });
        }
        for unit in text.encode_utf16() {
            out.extend_from_slice(&if little {
                unit.to_le_bytes()
            } else {
                unit.to_be_bytes()
            });
        }
        return (out, false);
    }
    let (bytes, _, bad) = to.encode(text);
    let mut out = Vec::new();
    if bom && to == encoding_rs::UTF_8 {
        out.extend_from_slice(b"\xEF\xBB\xBF");
    }
    out.extend_from_slice(&bytes);
    (out, bad)
}

/// Normalise every line ending.
pub fn set_line_endings(text: &str, crlf: bool) -> String {
    let unix = text.replace("\r\n", "\n").replace('\r', "\n");
    if crlf {
        unix.replace('\n', "\r\n")
    } else {
        unix
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
