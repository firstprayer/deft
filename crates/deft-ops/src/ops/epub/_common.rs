//! Helpers shared by the epub operations.

use deft_core::prelude::*;
use deft_tools_doc::epub::{Book, read};

pub const IN: &[Format] = &[Format::Epub];

pub fn load(input: &InputFile) -> Result<Book> {
    read(input.bytes()?).map_err(|mut e| {
        e.message = format!("{}: {}", input.display, e.message);
        e
    })
}
