use super::_common::IN;
use deft_core::prelude::*;
use deft_tools_pdf::{PageSetup, PdfImage};

pub static OP: Op = Op {
    id: "image.to-pdf",
    summary: "One page per image",
    input: Input::all(IN, 1, "image"),
    output: Output::one(&[Format::Pdf], "pdf"),
    params: &[
        param(
            "page-size",
            Kind::Choice(&["a4", "letter", "legal", "a3", "a5", "fit"]),
            "Paper, or fit to each image",
        )
        .default("a4"),
        param(
            "orientation",
            Kind::Choice(&["auto", "portrait", "landscape"]),
            "Page orientation",
        )
        .default("auto"),
        param("margin", Kind::Length, "Margin around each image").default("0mm"),
    ],
    mounts: &["pdf from-images"],
    converts: &[Edge {
        from: deft_tools_image::READABLE,
        to: &[Format::Pdf],
        quality: Quality::Lossless,
    }],
    examples: &[
        "deft image to-pdf photo-1.jpg photo-2.jpg -o album.pdf",
        "deft pdf from-images scans/*.jpg --margin 10mm -o scans.pdf",
    ],
    tags: &["jpg to pdf", "png to pdf", "scan", "album"],
    result: &["pages"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let max = job.ctx.limits.max_pixels;
    let images = job
        .inputs
        .iter()
        .map(|i| match i.require_format()? {
            Format::Heic | Format::Avif => {
                Ok(PdfImage::from_image(&super::_common::decode(job, i)?))
            }
            format => PdfImage::from_bytes(i.bytes()?, format, max),
        })
        .collect::<Result<Vec<_>>>()?;
    let size = p.text("page-size");
    let paper = deft_tools_pdf::PAPER
        .iter()
        .find(|x| x.0 == size)
        .map(|x| (x.1, x.2));
    let landscape = match p.text("orientation").as_str() {
        "portrait" => Some(false),
        "landscape" => Some(true),
        _ => None,
    };
    let mut doc = deft_tools_pdf::images_to_pdf(
        &images,
        PageSetup {
            paper,
            landscape,
            margin: p.points("margin"),
        },
    )?;
    let bytes = deft_tools_pdf::save(&mut doc)?;
    let n = images.len();
    Ok(OpResult::one(Produced::bytes(bytes, Format::Pdf))
        .fact("pages", n)
        .note(format!("{n} page{}", if n == 1 { "" } else { "s" })))
}
