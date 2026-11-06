//! Helpers shared by the doc operations.

use deft_core::prelude::*;
use deft_tools_doc::Document;

pub const IN: &[Format] = &[Format::Docx, Format::Odt, Format::Rtf];
pub const IMAGES: ParamSpec = param("images", Kind::Path, "Folder for embedded pictures");

pub fn load(input: &InputFile) -> Result<Document> {
    let bytes = input.bytes()?;
    match input.require_format()? {
        Format::Docx => deft_tools_doc::docx::read(bytes),
        Format::Odt => deft_tools_doc::odf::read(bytes),
        _ => Ok(deft_tools_doc::rtf::read(bytes)),
    }
    .map_err(|mut e| {
        e.message = format!("{}: {}", input.display, e.message);
        e
    })
}
