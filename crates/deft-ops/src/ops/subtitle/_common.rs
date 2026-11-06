//! Helpers shared by the subtitle operations.

use deft_core::prelude::*;
use deft_tools_text::Cue;

pub const IN: &[Format] = &[Format::Srt, Format::Vtt, Format::Ass];

pub fn read(input: &InputFile) -> Result<Vec<Cue>> {
    let text = String::from_utf8_lossy(input.bytes()?).to_string();
    let cues = if input.format == Some(Format::Ass) {
        deft_tools_text::parse_ass(&text)
    } else {
        deft_tools_text::parse_subtitles(&text)
    };
    if cues.is_empty() {
        return Err(err(
            ErrorCode::InputCorrupt,
            format!("{}: no cues found", input.display),
        ));
    }
    Ok(cues)
}

pub fn render(cues: &[Cue], format: Format) -> Vec<u8> {
    match format {
        Format::Vtt => deft_tools_text::cues_to_vtt(cues),
        Format::Txt => deft_tools_text::cues_to_text(cues),
        Format::Md => deft_tools_text::cues_to_markdown(cues, None),
        _ => deft_tools_text::cues_to_srt(cues),
    }
    .into_bytes()
}
