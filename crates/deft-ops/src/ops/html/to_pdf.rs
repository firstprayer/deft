use super::_common::{IN, file_url, text};
use crate::generated::md::_common::{FONT, FONT_SIZE, MARGIN, PAGE_SIZE, markdown_pdf};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "html.to-pdf",
    summary: "Make a PDF; --browser keeps the CSS",
    details: &[
        "By default the content is typeset without CSS. --browser prints through Chrome.",
        "With --browser the page cannot reach the network, so it must be self-contained.",
    ],
    input: Input::each(IN, "html"),
    output: Output::one(&[Format::Pdf], "pdf"),
    params: &[
        PAGE_SIZE,
        MARGIN,
        FONT_SIZE,
        FONT,
        param(
            "browser",
            Kind::Flag,
            "Print through Chrome, keeping the CSS",
        ),
    ],
    converts: &[Edge {
        from: IN,
        to: &[Format::Pdf],
        quality: Quality::Approximate,
    }],
    stability: Stability::Beta,
    examples: &["deft html to-pdf page.html -o page.pdf"],
    tags: &["html to pdf", "print page", "wkhtmltopdf"],
    result: &["pages"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    if job.params.flag("browser") {
        let program = deft_engines::require(job.ctx, "chrome", OP.id)?;
        let out = job.ctx.temp_path("page.pdf");
        let mut args = deft_engines::browser_args(&job.ctx.temp_path("profile"));
        args.push("--no-pdf-header-footer".to_string());
        args.push(format!("--print-to-pdf={}", out.display()));
        args.push(file_url(job)?);
        deft_engines::run_browser(job.ctx, &program, &args, &out)?;
        return Ok(OpResult::one(Produced::file(out, Some(Format::Pdf))));
    }
    let md = deft_tools_doc::html::to_markdown(&text(job.input())?)?;
    let base = job.input().path.as_ref().and_then(|p| p.parent());
    markdown_pdf(job, &md, base)
}
