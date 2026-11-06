//! Helpers shared by the slides operations.

use deft_core::prelude::*;
use deft_tools_doc::Document;
use deft_tools_doc::pptx::{Slide, read, to_document};

pub const IN: &[Format] = &[Format::Pptx];
pub const NOTES: ParamSpec = param("notes", Kind::Flag, "Include speaker notes");

pub fn load(job: &Job) -> Result<(Vec<Slide>, Document)> {
    let (slides, images) = read(job.input().bytes()?).map_err(|mut e| {
        e.message = format!("{}: {}", job.input().display, e.message);
        e
    })?;
    let doc = to_document(&slides, images, job.params.flag("notes"));
    Ok((slides, doc))
}
