//! Helpers shared by the calendar operations.

use deft_core::prelude::*;
use deft_tools_data::ical::{self, Event};

pub fn load(input: &InputFile) -> Result<Vec<Event>> {
    let text = String::from_utf8_lossy(input.bytes()?);
    let events = ical::parse(&text);
    if events.is_empty() {
        return Err(err(
            ErrorCode::InputCorrupt,
            format!("{}: no events found", input.display),
        ));
    }
    Ok(events)
}
