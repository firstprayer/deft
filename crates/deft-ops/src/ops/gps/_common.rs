//! Helpers shared by the gps operations.

use deft_core::prelude::*;
use deft_tools_data::geo::{self, Geo};

pub const IN: &[Format] = &[Format::Gpx, Format::Kml];

pub fn load(input: &InputFile) -> Result<Geo> {
    let text = String::from_utf8_lossy(input.bytes()?);
    let parsed = match input.require_format()? {
        Format::Kml => geo::parse_kml(&text),
        _ => geo::parse_gpx(&text),
    };
    let geo =
        parsed.map_err(|e| err(ErrorCode::InputCorrupt, format!("{}: {e}", input.display)))?;
    if geo.tracks.is_empty() && geo.places.is_empty() && geo.areas.is_empty() {
        return Err(err(
            ErrorCode::InputCorrupt,
            format!("{}: no tracks or places found", input.display),
        ));
    }
    Ok(geo)
}
