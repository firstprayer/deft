use super::_common::{PASSWORD, open};
use deft_core::prelude::*;
use serde_json::json;

pub static OP: Op = Op {
    id: "pdf.search",
    summary: "Find text: --text invoice",
    details: &["Ignores case. Several files are searched one by one. Scans need pdf ocr first."],
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::report(),
    params: &[
        param("text", Kind::Text, "What to look for").required(),
        PASSWORD,
    ],
    examples: &["deft pdf search report.pdf --text 'page 2'"],
    tags: &["grep", "find", "contains", "look up"],
    result: &["matches", "pages"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let doc = open(job.input(), job.params)?;
    let needle = job.params.text("text");
    let mut pages = Vec::new();
    let mut matches = 0;
    let mut blank = 0;
    let mut all = String::new();
    let count = deft_tools_pdf::page_ids(&doc).len();
    let indexes: Vec<usize> = (0..count).collect();
    deft_tools_pdf::text::reader(&doc)?.each_page_text(&indexes, |i, page| {
        let flat = page.flat();
        blank += usize::from(flat.is_empty());
        all.extend(flat.iter());
        let hits = page.find(&needle);
        if let Some(first) = hits.first() {
            matches += hits.len();
            let start = first.start.saturating_sub(40);
            let snippet: String = flat[start..].iter().take(100).collect();
            pages.push(json!({"page": i + 1, "hits": hits.len(), "snippet": snippet}));
        }
    });
    let note = format!("{matches} matches on {} pages", pages.len());
    let mut res = OpResult::report(facts![("matches", matches), ("pages", pages)], note);
    if blank == count {
        res = res.warn("no text found; a scan needs pdf ocr first");
    } else if blank > 0 {
        res = res.warn(format!(
            "{blank} of {count} pages have no text; a scan needs pdf ocr first"
        ));
    }
    let lost = deft_tools_pdf::text::undecoded(&all);
    if lost > 0.05 {
        res = res.warn(format!(
            "{:.0}% of the text could not be decoded, so matches may be missing",
            lost * 100.0
        ));
    }
    Ok(res)
}
