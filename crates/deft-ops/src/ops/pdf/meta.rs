use super::_common::{PASSWORD, open, write};
use deft_core::prelude::*;
use serde_json::Value as Json;

pub static OP: Op = Op {
    id: "pdf.meta",
    summary: "Read or set metadata and bookmarks",
    details: &["With no options it prints the fields. With options it writes a copy."],
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::one(&[Format::Pdf], "pdf"),
    params: &[
        param("title", Kind::Text, "Document title"),
        param("author", Kind::Text, "Author"),
        param("subject", Kind::Text, "Subject"),
        param("keywords", Kind::Text, "Keywords"),
        PASSWORD,
    ],
    examples: &["deft pdf meta report.pdf --title 'Q3 report' --author Ada -o titled.pdf"],
    tags: &[
        "metadata",
        "title",
        "author",
        "properties",
        "document info",
        "bookmarks",
        "outline",
    ],
    result: &[
        "title", "author", "subject", "keywords", "created", "modified", "outline",
    ],
    suffix: Some("meta"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let mut doc = open(job.input(), p)?;
    let mut fields: Vec<(&[u8], String)> = Vec::new();
    for (option, key) in [
        ("title", &b"Title"[..]),
        ("author", b"Author"),
        ("subject", b"Subject"),
        ("keywords", b"Keywords"),
    ] {
        if let Some(v) = p.opt_text(option) {
            fields.push((key, v));
        }
    }
    if fields.is_empty() {
        let map: serde_json::Map<String, Json> = deft_tools_pdf::meta::read(&doc)
            .into_iter()
            .map(|(k, v)| (k.to_string(), Json::String(v)))
            .collect();
        let n = map.len();
        let mut map = map;
        if let Ok(toc) = doc.get_toc() {
            let outline: Vec<Json> = toc
                .toc
                .iter()
                .map(|e| serde_json::json!({"level": e.level, "title": e.title, "page": e.page}))
                .collect();
            map.insert("outline".to_string(), Json::Array(outline));
        }
        return Ok(OpResult::report(map, format!("{n} fields")));
    }
    deft_tools_pdf::meta::write(&mut doc, &fields)?;
    Ok(OpResult::one(write(&mut doc)?))
}
