use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "pdf.info",
    summary: "Pages, size, metadata, fonts, images",
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::report(),
    params: &[super::_common::PASSWORD],
    examples: &["deft pdf info report.pdf"],
    tags: &[
        "inspect",
        "metadata",
        "page count",
        "how many pages",
        "details",
    ],
    result: &[
        "pages", "width", "height", "paper", "title", "author", "images", "fonts", "forms",
    ],
    errors: &[ErrorCode::InputEncrypted],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let doc = super::_common::open(job.input(), job.params)?;
    let i = deft_tools_pdf::info(&doc);
    let paper = i
        .paper
        .map(str::to_string)
        .unwrap_or_else(|| format!("{}x{}pt", i.width, i.height));
    let note = format!(
        "{}, {paper}, PDF {}",
        super::_common::plural(i.pages, "page"),
        i.version
    );
    let facts = facts![
        ("pages", i.pages),
        ("version", i.version),
        ("width", i.width),
        ("height", i.height),
        ("paper", i.paper),
        ("title", i.title),
        ("author", i.author),
        ("producer", i.producer),
        ("images", i.images),
        ("fonts", i.fonts),
        ("forms", i.forms),
    ];
    let res = OpResult::report(facts, note);
    Ok(if deft_tools_pdf::forms::is_dynamic_xfa(&doc) {
        res.warn(deft_tools_pdf::forms::XFA_WARNING)
    } else {
        res
    })
}
