use super::_common::{PASSWORD, open, plural, resolve_set, write};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "pdf.rotate",
    summary: "Turn pages clockwise: --angle 90, --pages",
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::one(&[Format::Pdf], "pdf"),
    params: &[
        param(
            "angle",
            Kind::Float {
                min: -360.0,
                max: 360.0,
            },
            "Degrees clockwise: 90, 180, 270, -90",
        )
        .default("90"),
        param("pages", Kind::Pages, "Pages to turn").default("all"),
        PASSWORD,
    ],
    examples: &[
        "deft pdf rotate three-pages.pdf --angle 180 -o turned.pdf",
        "deft pdf rotate report.pdf --pages 1 --angle -90",
    ],
    tags: &["turn", "orientation", "landscape"],
    result: &["rotated"],
    suffix: Some("rotated"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let mut doc = open(job.input(), job.params)?;
    let indexes = resolve_set(
        &job.params.pages("pages"),
        deft_tools_pdf::page_ids(&doc).len(),
    )?;
    let angle = right_angle(job.params.float("angle"))?;
    deft_tools_pdf::rotate(&mut doc, &indexes, angle);
    Ok(OpResult::one(write(&mut doc)?)
        .fact("rotated", indexes.len())
        .note(format!(
            "{} turned {angle} degrees",
            plural(indexes.len(), "page")
        )))
}
