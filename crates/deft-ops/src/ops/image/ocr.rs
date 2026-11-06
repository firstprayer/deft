use super::_common::{IN, open};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "image.ocr",
    summary: "Read the text in an image (tesseract)",
    details: &["--lang takes tesseract codes such as eng, deu, chi_sim, or eng+deu."],
    input: Input::each(IN, "image"),
    output: Output::one(&[Format::Txt], "txt"),
    params: &[param("lang", Kind::Text, "Language codes").default("eng")],
    engines: &["tesseract"],
    examples: &["deft image ocr page.png -o page.txt"],
    tags: &[
        "text recognition",
        "read text",
        "screenshot to text",
        "extract text",
        "tesseract",
    ],
    result: &["chars"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let (img, _) = open(job)?;
    let path = job.ctx.temp_path("page.png");
    let bytes = deft_tools_image::encode(&img, Format::Png, Default::default())?;
    std::fs::write(&path, bytes)?;
    let text = deft_engines::ocr_text(job.ctx, &path, &job.params.text("lang"), OP.id)?;
    let chars = text.trim().chars().count();
    let mut res = OpResult::one(Produced::bytes(text.into_bytes(), Format::Txt))
        .fact("chars", chars)
        .note(format!("{chars} characters"));
    if chars == 0 {
        res = res.warn("no text found");
    }
    Ok(res)
}
