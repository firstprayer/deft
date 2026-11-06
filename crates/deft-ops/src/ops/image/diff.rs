use super::_common::{FUZZ, IN, decode};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "image.diff",
    summary: "Compare two images; marks changed pixels in red",
    input: Input::all(IN, 2, "image"),
    output: Output::one(&[Format::Png], "png"),
    params: &[FUZZ.default("2")],
    examples: &["deft image diff photo-1.jpg photo-1.jpg -o same.png"],
    tags: &["compare", "visual regression", "what changed", "similarity"],
    result: &["identical", "changed", "mean"],
    suffix: Some("diff"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    if job.inputs.len() != 2 {
        return Err(err(ErrorCode::BadOption, "give exactly two images"));
    }
    let a = decode(job, &job.inputs[0])?;
    let b = decode(job, &job.inputs[1])?;
    let d = deft_tools_image::edit::compare(&a, &b, job.params.float("fuzz") as f32)?;
    let bytes = deft_tools_image::encode(&d.picture, Format::Png, Default::default())?;
    let percent = d.changed * 100.0;
    Ok(OpResult::one(Produced::bytes(bytes, Format::Png))
        .fact("identical", d.mean == 0.0)
        .fact("changed", d.changed)
        .fact("mean", d.mean)
        .note(format!("{percent:.2}% of pixels differ")))
}
