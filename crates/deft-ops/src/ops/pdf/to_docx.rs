use super::_common::{PASSWORD, open, resolve};
use deft_core::prelude::*;
use deft_tools_doc::{Block, Document, Image, Span};
use deft_tools_pdf::extract::Extracted;

pub static OP: Op = Op {
    id: "pdf.to-docx",
    summary: "Editable Word file, layout not kept",
    details: &[
        "Layout is not preserved: columns, tables and fonts become plain paragraphs.",
        "A scan has no text; run pdf ocr first.",
    ],
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::one(&[Format::Docx], "docx"),
    params: &[
        param("pages", Kind::Pages, "Pages to read").default("all"),
        PASSWORD,
    ],
    mounts: &["doc from-pdf"],
    converts: &[Edge {
        from: &[Format::Pdf],
        to: &[Format::Docx],
        quality: Quality::Approximate,
    }],
    stability: Stability::Beta,
    examples: &["deft pdf to-docx report.pdf -o report.docx"],
    tags: &["pdf to word", "editable", "doc", "office"],
    result: &["words", "images"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let doc = open(job.input(), job.params)?;
    let count = deft_tools_pdf::page_ids(&doc).len();
    let indexes = resolve(&job.params.pages("pages"), count)?;
    let mut blocks = Vec::new();
    let mut images = Vec::new();
    for (n, &i) in indexes.iter().enumerate() {
        if n > 0 {
            blocks.push(Block::PageBreak);
        }
        for p in deft_tools_pdf::reflow(&deft_tools_pdf::extract_text(&doc, &[i])?) {
            blocks.push(Block::Paragraph(vec![Span::plain(p)]));
        }
        for (_, img) in deft_tools_pdf::extract::images(&doc, &[i]).images {
            let (bytes, ext) = match img {
                Extracted::Jpeg(b) => (b, "jpg"),
                Extracted::Pixels(p) => (
                    deft_tools_image::encode(&p, Format::Png, Default::default())?,
                    "png",
                ),
            };
            let name = format!("image{}.{ext}", images.len() + 1);
            blocks.push(Block::Paragraph(vec![Span {
                image: Some(name.clone()),
                ..Default::default()
            }]));
            images.push(Image { name, bytes });
        }
    }
    let info = deft_tools_pdf::info(&doc);
    let out = Document {
        title: info.title,
        author: info.author,
        blocks,
        images,
    };
    let words = out.words();
    let n = out.images.len();
    let mut res = OpResult::one(Produced::bytes(
        deft_tools_doc::docx::write(&out)?,
        Format::Docx,
    ))
    .fact("words", words)
    .fact("images", n)
    .note(format!("{words} words, {n} images"));
    if words == 0 {
        res = res.warn("no text found; the PDF may be a scan: run pdf ocr first");
    }
    Ok(res)
}
