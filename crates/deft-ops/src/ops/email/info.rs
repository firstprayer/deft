use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "email.info",
    summary: "From, to, subject, date, attachments",
    input: Input::each(&[Format::Eml], "eml"),
    output: Output::report(),
    examples: &["deft email info message.eml"],
    tags: &["headers", "sender", "subject"],
    result: &["from", "to", "subject", "date", "attachments"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let m = deft_tools_doc::email::read(job.input().bytes()?)?;
    let names: Vec<&str> = m.attachments.iter().map(|a| a.name.as_str()).collect();
    let note = m.subject.clone();
    Ok(OpResult::report(
        facts![
            ("from", m.from.clone()),
            ("to", m.to.clone()),
            ("subject", m.subject.clone()),
            ("date", m.date.clone()),
            ("attachments", names)
        ],
        note,
    ))
}
