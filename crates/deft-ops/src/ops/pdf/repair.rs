use super::_common::plural;
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "pdf.repair",
    summary: "Rebuild a damaged PDF (Ghostscript)",
    details: &["Rewrites everything Ghostscript can still read; what is lost stays lost."],
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::one(&[Format::Pdf], "pdf"),
    engines: &["gs"],
    examples: &["deft pdf repair corrupt/broken.pdf -o fixed.pdf"],
    result: &["pages"],
    tags: &["fix", "corrupt", "damaged", "recover", "broken xref"],
    suffix: Some("repaired"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let source = job.input().local_path(job.ctx)?;
    let done = super::_gs::rewrite(job, OP.id, &source, &[])?;
    // Ghostscript succeeds even when it recovers only one empty page.
    let Some(Data::File(path)) = done.outputs.first().map(|o| &o.data) else {
        return Ok(done);
    };
    let bytes = std::fs::read(path)?;
    let renderer = deft_tools_pdf::Renderer::new(&bytes)?;
    let pages = renderer.page_count();
    let limit = job.ctx.limits.max_pixels;
    if pages == 1 && deft_tools_pdf::pages::content_region(&renderer, 0, limit)?.is_none() {
        return Err(err(
            ErrorCode::InputCorrupt,
            format!("{}: nothing could be recovered", job.input().display),
        ));
    }
    Ok(done.fact("pages", pages).note(plural(pages, "page")))
}
