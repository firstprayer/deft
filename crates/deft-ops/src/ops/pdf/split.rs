use super::_common::{PASSWORD, open, plural, resolve, write};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "pdf.split",
    summary: "Split: per page, --every N, --ranges",
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::many(&[Format::Pdf], "pdf"),
    params: &[
        param(
            "every",
            Kind::Int {
                min: 1,
                max: 100_000,
            },
            "Pages per output file",
        )
        .default("1"),
        param(
            "ranges",
            Kind::Text,
            "Comma-separated page ranges, one output each: 1-3,4-",
        ),
        PASSWORD,
    ],
    examples: &[
        "deft pdf split three-pages.pdf -o pages/",
        "deft pdf split report.pdf --ranges 1,2- -o parts/",
    ],
    tags: &["separate", "burst", "extract pages", "break"],
    result: &["files"],
    suffix: Some("pages"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let input = job.input();
    let mut doc = open(input, job.params)?;
    let count = deft_tools_pdf::page_ids(&doc).len();
    let groups: Vec<Vec<usize>> = match job.params.opt_text("ranges") {
        Some(r) => r
            .split(',')
            .map(|part| {
                PageSet::parse(part)
                    .map_err(|m| bad_value("ranges", m))
                    .and_then(|p| resolve(&p, count))
            })
            .collect::<Result<_>>()?,
        None => {
            let every = job.params.int("every") as usize;
            (0..count)
                .collect::<Vec<_>>()
                .chunks(every)
                .map(<[usize]>::to_vec)
                .collect()
        }
    };
    let stem = input.stem();
    let mut outputs = Vec::new();
    let parts = deft_tools_pdf::split(&mut doc, &groups)?;
    for (n, (pages, mut part)) in groups.iter().zip(parts).enumerate() {
        outputs.push(
            write(&mut part)?
                .named(format!("{stem}-{:03}.pdf", n + 1))
                .fact("pages", pages.len()),
        );
    }
    let files = outputs.len();
    Ok(OpResult::many(outputs)
        .fact("files", files)
        .note(plural(files, "file")))
}
