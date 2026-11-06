use super::_common::{IN, read};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "subtitle.info",
    summary: "Number of lines, start and end time",
    input: Input::each(IN, "subtitle"),
    output: Output::report(),
    examples: &["deft subtitle info talk.srt"],
    tags: &["count", "length", "duration"],
    result: &["cues", "first", "last", "chars"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let cues = read(job.input())?;
    let (first, last) = (cues[0].start, cues.last().map(|c| c.end).unwrap_or(0.0));
    let chars: usize = cues.iter().map(|c| c.text.chars().count()).sum();
    let note = format!(
        "{} lines, {} to {}",
        cues.len(),
        format_duration(first),
        format_duration(last)
    );
    Ok(OpResult::report(
        facts![
            ("cues", cues.len()),
            ("first", first),
            ("last", last),
            ("chars", chars)
        ],
        note,
    ))
}
