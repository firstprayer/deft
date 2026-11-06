use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "md.to-html",
    summary: "Render to a styled HTML page, or --fragment",
    input: Input::each(&[Format::Md, Format::Txt], "md"),
    output: Output::one(&[Format::Html], "html"),
    params: &[
        param("fragment", Kind::Flag, "Body only, no <html> page"),
        param("title", Kind::Text, "Page title (default: first heading)"),
    ],
    converts: &[Edge {
        from: &[Format::Md],
        to: &[Format::Html],
        quality: Quality::Faithful,
    }],
    examples: &[
        "deft md to-html notes.md -o notes.html",
        "deft md to-html notes.md --fragment -o -",
    ],
    tags: &["markdown to html", "render", "web page"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let md = String::from_utf8_lossy(job.input().bytes()?).to_string();
    let html = deft_tools_text::to_html(
        &md,
        !job.params.flag("fragment"),
        job.params.opt_text("title").as_deref(),
    );
    Ok(OpResult::one(Produced::bytes(
        html.into_bytes(),
        Format::Html,
    )))
}
