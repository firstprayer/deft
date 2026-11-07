//! Helpers shared by the doc operations.

use deft_core::prelude::*;
use deft_tools_doc::Document;

pub const IN: &[Format] = &[Format::Docx];
pub const IMAGES: ParamSpec = param("images", Kind::Path, "Folder for embedded pictures");

pub fn load(input: &InputFile) -> Result<Document> {
    deft_tools_doc::docx::read(input.bytes()?).map_err(|mut e| {
        e.message = format!("{}: {}", input.display, e.message);
        e
    })
}
