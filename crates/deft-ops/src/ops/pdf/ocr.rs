use super::_common::{PASSWORD, open, plural, resolve_set};
use deft_core::prelude::*;
use deft_tools_pdf::lopdf::Document;

pub static OP: Op = Op {
    id: "pdf.ocr",
    summary: "Make a scan searchable",
    details: &[
        "The pages stay as they are. --lang takes tesseract codes such as eng or chi_sim+eng.",
        "Writing to .txt gives the recognised text instead.",
    ],
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::one(&[Format::Pdf, Format::Txt], "pdf|txt"),
    params: &[
        param("lang", Kind::Text, "Language codes").default("eng"),
        param(
            "dpi",
            Kind::Int { min: 72, max: 600 },
            "Resolution to read at",
        )
        .default("200"),
        param("pages", Kind::Pages, "Pages to read").default("all"),
        PASSWORD,
    ],
    engines: &["tesseract"],
    examples: &[
        "deft pdf ocr scan.pdf -o searchable.pdf",
        "deft pdf ocr scan.pdf --pages 1 --dpi 150 -o scan.txt",
    ],
    tags: &[
        "searchable",
        "scan",
        "recognize text",
        "tesseract",
        "image pdf",
    ],
    result: &["pages", "chars"],
    suffix: Some("ocr"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let mut doc = open(job.input(), p)?;
    let bytes = deft_tools_pdf::save(&mut doc)?;
    let count = deft_tools_pdf::page_ids(&doc).len();
    let indexes = resolve_set(&p.pages("pages"), count)?;
    let renderer = deft_tools_pdf::Renderer::new(&bytes)?;
    let lang = p.text("lang");
    let dpi = p.int("dpi");
    let target = job.target(Format::Pdf);
    let mut text = String::new();
    let mut layers = Vec::new();
    for &i in &indexes {
        let img = renderer.render(i, dpi as f64, job.ctx.limits.max_pixels)?;
        let page = job.ctx.temp_path("page.png");
        let png = deft_tools_image::encode(
            &deft_tools_image::image::DynamicImage::ImageRgb8(img),
            Format::Png,
            Default::default(),
        )?;
        std::fs::write(&page, png)?;
        if target == Format::Txt {
            text.push_str(&deft_engines::ocr_text(job.ctx, &page, &lang, OP.id)?);
            text.push('\x0c');
        } else {
            let pdf = deft_engines::ocr_layer(job.ctx, &page, &lang, dpi, OP.id)?;
            layers.push(Document::load(&pdf).ctx("tesseract output")?);
        }
    }
    if target == Format::Txt {
        let chars = text.trim().chars().count();
        return Ok(
            OpResult::one(Produced::bytes(text.into_bytes(), Format::Txt))
                .fact("pages", indexes.len())
                .fact("chars", chars)
                .note(format!("{chars} characters")),
        );
    }
    let mut all = vec![doc];
    all.extend(layers);
    let mut merged = deft_tools_pdf::merge(all)?;
    let ids = deft_tools_pdf::page_ids(&merged);
    for (n, &i) in indexes.iter().enumerate() {
        deft_tools_pdf::pages::overlay_page(&mut merged, ids[i], ids[count + n])?;
    }
    let keep: Vec<usize> = (0..count).collect();
    deft_tools_pdf::select(&mut merged, &keep)?;
    let out = deft_tools_pdf::save(&mut merged)?;
    Ok(OpResult::one(Produced::bytes(out, Format::Pdf))
        .fact("pages", indexes.len())
        .note(plural(indexes.len(), "page")))
}
