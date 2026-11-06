use super::_common::{IN, read, render};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "subtitle.shift",
    summary: "Move all timing: --by 2.5 (later) or --by -1 (earlier)",
    input: Input::each(IN, "subtitle"),
    output: Output::one(IN, "subtitle"),
    params: &[param(
        "by",
        Kind::Float {
            min: -86400.0,
            max: 86400.0,
        },
        "Seconds; negative means earlier",
    )
    .required()],
    examples: &[
        "deft subtitle shift talk.srt --by 1.5 -o later.srt",
        "deft subtitle shift talk.vtt --by=-0.1 -o earlier.vtt",
    ],
    tags: &["sync", "delay", "offset", "out of sync", "timing"],
    result: &["cues"],
    suffix: Some("shifted"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let input = job.input();
    let cues = deft_tools_text::shift_cues(&read(input)?, job.params.float("by"));
    let format = job.target(input.require_format()?);
    Ok(
        OpResult::one(Produced::bytes(render(&cues, format), format))
            .fact("cues", cues.len())
            .note(format!("{} lines", cues.len())),
    )
}
