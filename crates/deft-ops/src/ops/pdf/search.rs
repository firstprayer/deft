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
    let needle = job.params.text("text").to_lowercase();
    let mut pages = Vec::new();
    let mut matches = 0;
    let count = deft_tools_pdf::page_ids(&doc).len();
    for i in 0..count {
        let text = deft_tools_pdf::extract_text(&doc, &[i])?;
        let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
        let lower = flat.to_lowercase();
        let hits = lower.matches(&needle).count();
        if hits > 0 {
            matches += hits;
            let at = lower.find(&needle).unwrap_or(0);
            let start = (0..=at.saturating_sub(40))
                .rev()
                .find(|i| lower.is_char_boundary(*i))
                .unwrap_or(0);
            let snippet: String = lower[start..].chars().take(100).collect();
            pages.push(json!({"page": i + 1, "hits": hits, "snippet": snippet}));
        }
    }
    let note = format!("{matches} matches on {} pages", pages.len());
    Ok(OpResult::report(
        facts![("matches", matches), ("pages", pages)],
        note,
    ))
}
