use super::_common::load;
use deft_core::prelude::*;

const IN: &[Format] = &[Format::Ics];

pub static OP: Op = Op {
    id: "calendar.to-csv",
    summary: "Turn an ICS calendar into a table, by start time",
    details: &["A repeating event stays one row, with its rule in the repeat column."],
    input: Input::each(IN, "ics"),
    output: Output::one(&[Format::Csv], "csv"),
    converts: &[Edge {
        from: IN,
        to: &[Format::Csv],
        quality: Quality::Faithful,
    }],
    examples: &["deft calendar to-csv events.ics -o events.csv"],
    tags: &["export calendar", "meetings", "schedule"],
    result: &["events"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let events = load(job.input())?;
    let bytes =
        deft_tools_data::ical::to_csv(&events).map_err(|e| err(ErrorCode::OutputUnwritable, e))?;
    Ok(OpResult::one(Produced::bytes(bytes, Format::Csv))
        .fact("events", events.len())
        .note(format!(
            "{} event{}",
            events.len(),
            if events.len() == 1 { "" } else { "s" }
        )))
}
