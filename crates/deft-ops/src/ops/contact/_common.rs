//! Helpers shared by the contact operations.

use deft_core::prelude::*;
use deft_tools_data::vcard::{self, Contact};

pub fn load(input: &InputFile) -> Result<Vec<Contact>> {
    let text = String::from_utf8_lossy(input.bytes()?);
    let contacts = vcard::parse(&text);
    if contacts.is_empty() {
        return Err(err(
            ErrorCode::InputCorrupt,
            format!("{}: no contacts found", input.display),
        ));
    }
    Ok(contacts)
}
