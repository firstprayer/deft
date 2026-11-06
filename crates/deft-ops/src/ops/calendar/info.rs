use super::_common::load;
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "calendar.info",
    summary: "Count events and show the first and last",
    input: Input::each(&[Format::Ics], "ics"),
    output: Output::report(),
    examples: &["deft calendar info events.ics"],
    tags: &["how many events", "schedule"],
    result: &["events", "first", "last"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let events = load(job.input())?;
    let first = events.first().map(|e| e.start.clone()).unwrap_or_default();
    let last = events.last().map(|e| e.start.clone()).unwrap_or_default();
    let note = format!(
        "{} event{}",
        events.len(),
        if events.len() == 1 { "" } else { "s" }
    );
    Ok(OpResult::report(
        facts![("events", events.len()), ("first", first), ("last", last)],
        note,
    ))
}
