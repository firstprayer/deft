use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "email.attachments",
    summary: "Save the attached files",
    input: Input::each(&[Format::Eml], "eml"),
    output: Output::many(&[], "files"),
    examples: &["deft email attachments message.eml -o files/"],
    tags: &["extract attachment", "save attachment"],
    result: &["files"],
    suffix: Some("attachments"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let m = deft_tools_doc::email::read(job.input().bytes()?)?;
    let mut outputs = Vec::new();
    for a in m.attachments {
        let path = job.ctx.temp_path("attachment");
        std::fs::write(&path, &a.bytes)?;
        outputs.push(Produced::file(path, None).named(a.name));
    }
    let n = outputs.len();
    Ok(OpResult::many(outputs)
        .fact("files", n)
        .note(format!("{n} files")))
}
