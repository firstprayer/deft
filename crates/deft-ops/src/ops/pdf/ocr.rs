use super::_common::{PASSWORD, open, page_list, plural, resolve_set};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "pdf.ocr",
    summary: "Read the text of a scanned PDF",
    details: &[
        "Writes the text to a .txt file; the PDF is not changed. --lang takes tesseract codes such as eng or chi_sim+eng.",
        "Pages turned sideways or upside down are read upright.",
        "Pages that already have text give that text; a scan with only a short stamp of text is read.",
        "Each page ends with a form feed.",
    ],
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::one(&[Format::Txt], "txt"),
    params: &[
        param("lang", Kind::Text, "Language codes").default("eng"),
        param(
            "dpi",
            Kind::Int { min: 72, max: 600 },
            "Resolution to read at",
        )
        .default("300"),
        param("pages", Kind::Pages, "Pages to read").default("all"),
        PASSWORD,
    ],
    engines: &["tesseract"],
    examples: &[
        "deft pdf ocr scan.pdf -o scan.txt",
        "deft pdf ocr scan.pdf --pages 1 --dpi 150 -o first-page.txt",
    ],
    tags: &[
        "scan to text",
        "scanned pdf text",
        "scan",
        "scanned",
        "recognize text",
        "tesseract",
        "image pdf",
    ],
    result: &["pages", "chars"],
    needs_output: true,
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let doc = open(job.input(), p)?;
    let count = deft_tools_pdf::page_ids(&doc).len();
    let indexes = resolve_set(&p.pages("pages"), count)?;
    let renderer = deft_tools_pdf::Renderer::of(&doc)?;
    let lang = p.text("lang");
    deft_engines::check_tesseract_lang(&lang).map_err(|m| bad_value("lang", m))?;
    let dpi = p.int("dpi");
    // A page with a real text layer gives that text, which is exact; reading
    // its picture would only guess. A scan with only a short stamp is read.
    let mut existing = Vec::with_capacity(indexes.len());
    renderer.each_page_text(&indexes, |_, page| {
        let searchable = page.chars() >= 20 && !page.is_scan();
        existing.push(searchable.then(|| page.text()));
    });
    let mut text = String::new();
    let mut skipped = Vec::new();
    let mut empty = Vec::new();
    let mut recognised = 0;
    let mut no_orientation = false;
    for (&i, own) in indexes.iter().zip(&existing) {
        if let Some(own) = own {
            skipped.push(i + 1);
            text.push_str(own);
            text.push_str("\n\x0c");
            continue;
        }
        let img = renderer.render(i, dpi as f64, job.ctx.limits.max_pixels)?;
        let page = job.ctx.temp_path("page.png");
        let png = deft_tools_image::encode(
            &deft_tools_image::image::DynamicImage::ImageRgb8(img),
            Format::Png,
            Default::default(),
        )?;
        std::fs::write(&page, png)?;
        let got = deft_engines::ocr_page(job.ctx, &page, &lang, dpi, OP.id)?;
        no_orientation |= got.no_orientation;
        let chars = got.text.chars().filter(|c| !c.is_whitespace()).count();
        recognised += chars;
        if chars == 0 {
            empty.push(i + 1);
        }
        text.push_str(&got.text);
        text.push('\x0c');
    }
    let mut warnings = Vec::new();
    if !skipped.is_empty() {
        let have = if skipped.len() == 1 { "has" } else { "have" };
        warnings.push(format!(
            "{} already {have} text, used as it is (page {})",
            plural(skipped.len(), "page"),
            page_list(&skipped)
        ));
    }
    let tried = indexes.len() - skipped.len();
    if tried > 0 && recognised == 0 {
        warnings.push(
            "no text recognised (handwriting, a photo or blank pages?); the text output is empty"
                .to_string(),
        );
    } else if !empty.is_empty() {
        warnings.push(format!(
            "no text recognised on {} of {tried} pages read (page {})",
            empty.len(),
            page_list(&empty)
        ));
    }
    if no_orientation {
        warnings.push(
            "tesseract has no orientation data (osd.traineddata), so pages on their side or \
             upside down were read as they are"
                .to_string(),
        );
    }
    let chars = text.trim().chars().count();
    let mut res = OpResult::one(Produced::bytes(text.into_bytes(), Format::Txt))
        .fact("pages", indexes.len())
        .fact("chars", chars)
        .note(format!("{chars} characters"));
    for w in warnings {
        res = res.warn(w);
    }
    Ok(res)
}
