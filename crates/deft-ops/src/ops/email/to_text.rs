use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "email.to-text",
    summary: "The message as text, with its headers",
    details: &["HTML-only messages are converted."],
    input: Input::each(&[Format::Eml], "eml"),
    output: Output::one(&[Format::Txt], "txt"),
    converts: &[Edge {
        from: &[Format::Eml],
        to: &[Format::Txt],
        quality: Quality::Faithful,
    }],
    examples: &["deft email to-text message.eml -o message.txt"],
    tags: &["read email", "body"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let m = deft_tools_doc::email::read(job.input().bytes()?)?;
    let cc = if m.cc.is_empty() {
        String::new()
    } else {
        format!("Cc: {}\n", m.cc)
    };
    let text = format!(
        "From: {}\nTo: {}\n{cc}Date: {}\nSubject: {}\n\n{}\n",
        m.from,
        m.to,
        m.date,
        m.subject,
        m.body.trim_end()
    );
    Ok(OpResult::one(Produced::bytes(
        text.into_bytes(),
        Format::Txt,
    )))
}
