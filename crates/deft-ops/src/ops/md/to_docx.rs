use super::_common::load_images;
use deft_core::prelude::*;
use deft_tools_doc::Document;

pub static OP: Op = Op {
    id: "md.to-docx",
    summary: "Write a Word document with headings, lists, tables, images",
    details: &["Relative image paths resolve against the Markdown file."],
    input: Input::each(&[Format::Md, Format::Txt], "md"),
    output: Output::one(&[Format::Docx], "docx"),
    params: &[
        param(
            "title",
            Kind::Text,
            "Document title (default: first heading)",
        ),
        param("author", Kind::Text, "Author"),
    ],
    mounts: &["doc from-md"],
    converts: &[Edge {
        from: &[Format::Md],
        to: &[Format::Docx],
        quality: Quality::Faithful,
    }],
    examples: &[
        "deft md to-docx notes.md -o notes.docx",
        "deft doc from-md notes.md --author Ada -o report.docx",
    ],
    tags: &["markdown to word", "docx", "word document", "office"],
    result: &["words"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let input = job.input();
    let md = String::from_utf8_lossy(input.bytes()?).to_string();
    let base = input.path.as_ref().and_then(|x| x.parent());
    let mut blocks = deft_tools_doc::markdown::parse(&md);
    let (images, warnings) = load_images(&mut blocks, base);
    let doc = Document {
        title: p.opt_text("title").or_else(|| deft_tools_text::title(&md)),
        author: p.opt_text("author"),
        blocks,
        images,
        ..Default::default()
    };
    let words = doc.words();
    let mut res = OpResult::one(Produced::bytes(
        deft_tools_doc::docx::write(&doc)?,
        Format::Docx,
    ))
    .fact("words", words)
    .note(format!("{words} words"));
    res.warnings = warnings;
    Ok(res)
}
