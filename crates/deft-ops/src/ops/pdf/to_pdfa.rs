use super::_common::PASSWORD;
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "pdf.to-pdfa",
    summary: "Archive as PDF/A-2 (Ghostscript)",
    details: &["Not validated: check with veraPDF when an archive demands strict conformance."],
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::one(&[Format::Pdf], "pdf"),
    params: &[PASSWORD],
    engines: &["gs"],
    stability: Stability::Beta,
    examples: &["deft pdf to-pdfa report.pdf -o archive.pdf"],
    tags: &["pdf/a", "pdfa", "archive", "long-term"],
    suffix: Some("pdfa"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let source = super::_gs::plain_copy(job)?;
    super::_gs::rewrite(
        job,
        OP.id,
        &source,
        &[
            "-dPDFA=2",
            "-dPDFACompatibilityPolicy=1",
            "-sColorConversionStrategy=RGB",
        ],
    )
}
