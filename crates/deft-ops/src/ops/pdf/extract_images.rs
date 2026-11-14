use super::_common::{PASSWORD, open, resolve};
use deft_core::prelude::*;
use deft_tools_pdf::extract::Extracted;

pub static OP: Op = Op {
    id: "pdf.extract-images",
    summary: "Save embedded pictures, unchanged",
    details: &[
        "JPEGs come out byte for byte; other images as PNG. Each image appears once.",
        "A CMYK JPEG becomes an RGB PNG: saved as it is, viewers would show it as a negative.",
    ],
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::many(&[Format::Png, Format::Jpeg], "image"),
    params: &[
        param("pages", Kind::Pages, "Pages to read").default("all"),
        PASSWORD,
    ],
    examples: &["deft pdf extract-images photos.pdf -o pictures/"],
    tags: &["images", "photos", "pictures", "pull images", "pdfimages"],
    result: &["files"],
    suffix: Some("images"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let doc = open(job.input(), job.params)?;
    let count = deft_tools_pdf::page_ids(&doc).len();
    let indexes = resolve(&job.params.pages("pages"), count)?;
    let found = deft_tools_pdf::extract::images(&doc, &indexes);
    let stem = job.input().stem();
    let mut outputs = Vec::new();
    for (n, (page, img)) in found.images.into_iter().enumerate() {
        let (bytes, format) = match img {
            Extracted::Jpeg(b) => (b, Format::Jpeg),
            Extracted::Pixels(i) => (
                deft_tools_image::encode(&i, Format::Png, Default::default())?,
                Format::Png,
            ),
        };
        let name = format!("{stem}-p{:03}-{:03}.{}", page + 1, n + 1, format.ext());
        outputs.push(Produced::bytes(bytes, format).named(name));
    }
    let n = outputs.len();
    let mut res = OpResult::many(outputs)
        .fact("files", n)
        .note(format!("{n} images"));
    if found.skipped > 0 {
        res = res.warn(format!(
            "{} images could not be decoded; pdf to-images renders whole pages",
            found.skipped
        ));
    }
    if n == 0 && found.skipped == 0 {
        res = res.warn("no embedded images found");
    }
    Ok(res)
}
