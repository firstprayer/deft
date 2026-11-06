use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "doc.replace",
    summary: "Find and replace text in Word, keeping the formatting",
    details: &["Exact, case-sensitive match. Covers the body, headers and footers."],
    input: Input::each(&[Format::Docx], "docx"),
    output: Output::one(&[Format::Docx], "docx"),
    params: &[
        param("find", Kind::Text, "Text to look for").required(),
        param("replace", Kind::Text, "Replacement").default(""),
    ],
    examples: &["deft doc replace letter.docx --find Ada --replace Grace -o changed.docx"],
    tags: &[
        "search and replace",
        "template",
        "mail merge",
        "fill placeholders",
    ],
    result: &["replaced"],
    suffix: Some("replaced"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let find = p.text("find");
    if find.is_empty() {
        return Err(bad_value("find", "must not be empty"));
    }
    let (bytes, n) =
        deft_tools_doc::docx::replace(job.input().bytes()?, &find, &p.text("replace"))?;
    let mut res = OpResult::one(Produced::bytes(bytes, Format::Docx))
        .fact("replaced", n)
        .note(format!("{n} replaced"));
    if n == 0 {
        res = res.warn(format!("{find:?} was not found"));
    }
    Ok(res)
}
