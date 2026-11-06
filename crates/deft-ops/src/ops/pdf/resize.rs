use super::_common::{PASSWORD, open, plural, resolve_set, write};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "pdf.resize",
    summary: "Fit pages to --page-size a4",
    details: &["Content is scaled and centred. Landscape pages stay landscape."],
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::one(&[Format::Pdf], "pdf"),
    params: &[
        param(
            "page-size",
            Kind::Choice(&["a3", "a4", "a5", "letter", "legal"]),
            "Paper",
        )
        .default("a4"),
        param("pages", Kind::Pages, "Pages to resize").default("all"),
        PASSWORD,
    ],
    examples: &["deft pdf resize report.pdf --page-size letter -o letter.pdf"],
    tags: &["scale", "paper size", "a4", "letter", "print"],
    result: &["resized"],
    suffix: Some("resized"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let mut doc = open(job.input(), p)?;
    let ids = deft_tools_pdf::page_ids(&doc);
    let indexes = resolve_set(&p.pages("pages"), ids.len())?;
    let name = p.text("page-size");
    let &(_, w, h) = deft_tools_pdf::PAPER
        .iter()
        .find(|(n, _, _)| n.eq_ignore_ascii_case(&name))
        .ok_or_else(|| bad_value("page-size", "unknown paper"))?;
    for &i in &indexes {
        let (pw, ph) = deft_tools_pdf::display_size(&doc, ids[i]);
        let (tw, th) = if pw > ph { (h, w) } else { (w, h) };
        deft_tools_pdf::pages::resize_page(&mut doc, ids[i], tw, th)?;
    }
    Ok(OpResult::one(write(&mut doc)?)
        .fact("resized", indexes.len())
        .note(plural(indexes.len(), "page")))
}
