use super::_common::{PASSWORD, open, plural, write};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "pdf.merge",
    summary: "Join PDFs in the given order",
    input: Input::all(&[Format::Pdf], 2, "pdf"),
    output: Output::one(&[Format::Pdf], "pdf"),
    params: &[PASSWORD],
    examples: &[
        "deft pdf merge report.pdf appendix.pdf -o all.pdf",
        "deft pdf merge *.pdf -o book.pdf",
    ],
    tags: &["combine", "join", "concatenate", "append"],
    result: &["pages"],
    suffix: Some("merged"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let docs = job
        .inputs
        .iter()
        .map(|i| {
            open(i, job.params).map_err(|e| {
                if e.code != ErrorCode::InputEncrypted || job.params.opt_text("password").is_none()
                {
                    return e;
                }
                // One --password serves every input.
                e.hint(
                    "--password is tried on every input; unlock the others first: \
                     deft pdf decrypt <file> --password <its password>",
                )
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let expected: usize = docs.iter().map(|d| deft_tools_pdf::page_ids(d).len()).sum();
    let mut merged = deft_tools_pdf::merge(docs)?;
    let pages = deft_tools_pdf::page_ids(&merged).len();
    if pages != expected {
        return Err(err(
            ErrorCode::Internal,
            format!("merged {pages} pages, expected {expected}"),
        ));
    }
    Ok(OpResult::one(write(&mut merged)?)
        .fact("pages", pages)
        .note(plural(pages, "page")))
}
