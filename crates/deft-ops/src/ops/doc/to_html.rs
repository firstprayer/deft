use super::_common::{IN, load};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "doc.to-html",
    summary: "Convert to a web page",
    details: &["Pictures are embedded in the page, so it is one file."],
    input: Input::each(IN, "doc"),
    output: Output::one(&[Format::Html], "html"),
    params: &[param("fragment", Kind::Flag, "Body only, no <html> page")],
    converts: &[Edge {
        from: IN,
        to: &[Format::Html],
        quality: Quality::Faithful,
    }],
    examples: &["deft doc to-html letter.docx -o letter.html"],
    tags: &["docx to html", "web page", "publish"],
    run,
    ..Op::DEFAULT
};

/// The type of a picture, for browsers. Word's own EMF and WMF have none.
fn media_type(name: &str) -> Option<&'static str> {
    let extension = name.rsplit('.').next()?.to_ascii_lowercase();
    Some(match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "bmp" => "image/bmp",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        _ => return None,
    })
}

fn run(job: &Job) -> Result<OpResult> {
    let doc = load(job.input())?;
    let md = deft_tools_doc::markdown::render(&doc);
    let mut html =
        deft_tools_text::to_html(&md, !job.params.flag("fragment"), doc.title.as_deref());
    let mut skipped = 0;
    for image in &doc.images {
        let Some(kind) = media_type(&image.name) else {
            skipped += 1;
            continue;
        };
        let data = deft_tools_file::base64_encode(&image.bytes);
        html = html.replace(
            &format!(
                "src=\"{}\"",
                deft_tools_doc::markdown::link_name(&image.name)
            ),
            &format!("src=\"data:{kind};base64,{data}\""),
        );
    }
    let mut res = OpResult::one(Produced::bytes(html.into_bytes(), Format::Html));
    res.warnings = doc.warnings;
    if skipped > 0 {
        res = res.warn(format!(
            "{skipped} pictures are in a format browsers cannot show"
        ));
    }
    Ok(res)
}
