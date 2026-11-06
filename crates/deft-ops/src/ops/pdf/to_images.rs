use super::_common::{PASSWORD, open, plural, resolve};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "pdf.to-images",
    summary: "Render pages: --dpi, --pages",
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::many(&[Format::Png, Format::Jpeg], "png|jpg"),
    params: &[
        param("to", Kind::Choice(&["png", "jpg"]), "Image format").default("png"),
        param("dpi", Kind::Int { min: 10, max: 1200 }, "Resolution").default("150"),
        param("quality", Kind::Int { min: 1, max: 100 }, "JPEG quality").default("85"),
        param("pages", Kind::Pages, "Pages to render").default("all"),
        PASSWORD,
    ],
    converts: &[Edge {
        from: &[Format::Pdf],
        to: &[Format::Png, Format::Jpeg],
        quality: Quality::Faithful,
    }],
    examples: &[
        "deft pdf to-images three-pages.pdf -o pages/",
        "deft pdf to-images report.pdf --pages 1 --to jpg --dpi 300 -o cover/",
    ],
    tags: &[
        "render",
        "png",
        "jpg",
        "jpeg",
        "screenshot",
        "thumbnail",
        "convert",
    ],
    result: &["files"],
    suffix: Some("images"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let input = job.input();
    let mut doc = open(input, p)?;
    let count = deft_tools_pdf::page_ids(&doc).len();
    let indexes = resolve(&p.pages("pages"), count)?;
    let format = Format::from_name(&p.text("to")).expect("choice");
    // The renderer cannot decrypt; give it the decrypted document when needed.
    let renderer = match deft_tools_pdf::Renderer::new(input.bytes()?) {
        Ok(r) if !p.has("password") => r,
        _ => deft_tools_pdf::Renderer::new(&deft_tools_pdf::save(&mut doc)?)?,
    };
    let stem = input.stem();
    let opt = deft_tools_image::Encode {
        quality: p.int("quality") as u8,
        ..Default::default()
    };
    let mut outputs = Vec::new();
    for &i in &indexes {
        let img = renderer.render(i, p.int("dpi") as f64, job.ctx.limits.max_pixels)?;
        let (w, h) = (img.width(), img.height());
        let bytes = deft_tools_image::encode(
            &deft_tools_image::image::DynamicImage::ImageRgb8(img),
            format,
            opt,
        )?;
        outputs.push(
            Produced::bytes(bytes, format)
                .named(format!("{stem}-{:03}.{}", i + 1, format.ext()))
                .fact("width", w)
                .fact("height", h)
                .fact("page", i + 1),
        );
    }
    let files = outputs.len();
    Ok(OpResult::many(outputs)
        .fact("files", files)
        .note(plural(files, "image")))
}
