use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "calendar.from-csv",
    summary: "Make an ICS calendar from a table",
    details: &["Needs summary and start columns, with dates like 2026-11-06 09:00."],
    input: Input::each(&[Format::Csv], "csv"),
    output: Output::one(&[Format::Ics], "ics"),
    examples: &["deft calendar from-csv schedule.csv -o plan.ics"],
    tags: &[
        "csv to ics",
        "excel to calendar",
        "import events",
        "schedule",
    ],
    result: &["events"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let corrupt = |e: String| err(ErrorCode::InputCorrupt, e);
    let events = deft_tools_data::ical::from_csv(job.input().bytes()?).map_err(corrupt)?;
    let text = deft_tools_data::ical::write(&events).map_err(corrupt)?;
    Ok(
        OpResult::one(Produced::bytes(text.into_bytes(), Format::Ics))
            .fact("events", events.len())
            .note(format!(
                "{} event{}",
                events.len(),
                if events.len() == 1 { "" } else { "s" }
            )),
    )
}
