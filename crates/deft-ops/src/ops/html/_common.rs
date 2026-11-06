//! Helpers shared by the html operations.

use deft_core::prelude::*;

pub const IN: &[Format] = &[Format::Html];

pub fn text(input: &InputFile) -> Result<String> {
    Ok(String::from_utf8_lossy(input.bytes()?).to_string())
}

/// The input as a `file://` address for a browser.
pub fn file_url(job: &Job) -> Result<String> {
    let path = std::path::absolute(job.input().local_path(job.ctx)?)?;
    Ok(format!(
        "file://{}",
        path.display().to_string().replace(' ', "%20")
    ))
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
