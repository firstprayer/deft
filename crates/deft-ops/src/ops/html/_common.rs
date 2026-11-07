//! Helpers shared by the html operations.

use deft_core::prelude::*;

pub const IN: &[Format] = &[Format::Html];

/// The charset a page declares in its first bytes.
fn declared_charset(bytes: &[u8]) -> Option<String> {
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(2048)]);
    let re = regex::Regex::new(r#"(?i)(?:charset|encoding)\s*=\s*["']?\s*([\w.:-]+)"#).ok()?;
    Some(re.captures(&head)?.get(1)?.as_str().to_string())
}

/// The page as text. UTF-8 is taken as it is; otherwise the page's own
/// declaration decides, then windows-1252, which is what browsers assume.
pub fn text(input: &InputFile) -> Result<String> {
    let bytes = input.bytes()?;
    let found = deft_tools_file::detect_encoding(bytes);
    let sure = found == deft_tools_file::Encoding::for_label(b"utf-8").expect("utf-8 exists")
        || deft_tools_file::Encoding::for_bom(bytes).is_some();
    let encoding = if sure {
        found
    } else {
        declared_charset(bytes)
            .and_then(|name| deft_tools_file::find_encoding(&name))
            .or_else(|| deft_tools_file::find_encoding("windows-1252"))
            .unwrap_or(found)
    };
    Ok(deft_tools_file::decode_text(bytes, encoding).0)
}

pub const MAIN: ParamSpec = param(
    "main",
    Kind::Flag,
    "Only the article, without site navigation",
);

/// The page, or just its article with `--main`.
pub fn content(job: &Job) -> Result<String> {
    let html = text(job.input())?;
    Ok(if job.params.flag("main") {
        deft_tools_doc::html::main_content(&html)
    } else {
        html
    })
}
