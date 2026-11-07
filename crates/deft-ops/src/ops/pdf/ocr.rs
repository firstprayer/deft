use super::_common::{PASSWORD, open, plural, resolve_set};
use deft_core::prelude::*;
use deft_tools_pdf::lopdf::Document;

pub static OP: Op = Op {
    id: "pdf.ocr",
    summary: "Make a scan searchable",
    details: &[
        "The pages stay as they are. --lang takes tesseract codes such as eng or chi_sim+eng.",
        "Pages that already have text are left as they are.",
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

/// Whether a page already has a text layer: OCR would add every word again.
fn has_text(text: &str) -> bool {
    text.chars()
        .filter(|c| !c.is_whitespace() && *c != '\u{FFFD}')
        .count()
        >= 20
}

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let doc = open(job.input(), p)?;
    let count = deft_tools_pdf::page_ids(&doc).len();
    let indexes = resolve_set(&p.pages("pages"), count)?;
    let renderer = deft_tools_pdf::Renderer::of(&doc)?;
    let lang = p.text("lang");
    deft_engines::check_tesseract_lang(&lang).map_err(|m| bad_value("lang", m))?;
    let dpi = p.int("dpi");
    let target = job.target(Format::Pdf);
    let existing = renderer.page_texts(&indexes);
    let mut text = String::new();
    let mut layers = Vec::new();
    let mut read = Vec::new();
    let mut skipped = Vec::new();
    for (&i, own) in indexes.iter().zip(&existing) {
        if has_text(own) {
            skipped.push(i + 1);
            if target == Format::Txt {
                text.push_str(own);
                text.push_str("\n\x0c");
            }
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
        if target == Format::Txt {
            text.push_str(&deft_engines::ocr_text(job.ctx, &page, &lang, OP.id)?);
            text.push('\x0c');
        } else {
            let pdf = deft_engines::ocr_layer(job.ctx, &page, &lang, dpi, OP.id)?;
            let layer = Document::load(&pdf).map_err(|e| {
                err(
                    ErrorCode::EngineFailed,
                    format!("tesseract wrote a PDF layer deft cannot read: {e}"),
                )
            })?;
            layers.push(layer);
            read.push(i);
        }
    }
    let warning = (!skipped.is_empty()).then(|| {
        let list: Vec<String> = skipped.iter().take(10).map(usize::to_string).collect();
        let more = if skipped.len() > 10 { ", ..." } else { "" };
        let have = if skipped.len() == 1 { "has" } else { "have" };
        format!(
            "{} already {have} text and {} not read again (page {}{more})",
            plural(skipped.len(), "page"),
            if skipped.len() == 1 { "was" } else { "were" },
            list.join(", ")
        )
    });
    let res = if target == Format::Txt {
        let chars = text.trim().chars().count();
        OpResult::one(Produced::bytes(text.into_bytes(), Format::Txt))
            .fact("pages", indexes.len())
            .fact("chars", chars)
            .note(format!("{chars} characters"))
    } else {
        let mut all = vec![doc];
        all.extend(layers);
        let mut merged = deft_tools_pdf::merge(all)?;
        let ids = deft_tools_pdf::page_ids(&merged);
        for (n, &i) in read.iter().enumerate() {
            deft_tools_pdf::pages::overlay_page(&mut merged, ids[i], ids[count + n])?;
        }
        let keep: Vec<usize> = (0..count).collect();
        deft_tools_pdf::select(&mut merged, &keep)?;
        OpResult::one(super::_common::write(&mut merged)?)
            .fact("pages", read.len())
            .note(plural(read.len(), "page"))
    };
    Ok(match warning {
        Some(w) => res.warn(w),
        None => res,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_page_with_a_text_layer_is_not_read_again() {
        assert!(super::has_text(
            "Trace-based Just-in-Time Type Specialization"
        ));
        assert!(!super::has_text("  12  "));
        assert!(!super::has_text(&"\u{FFFD}".repeat(40)));
    }
}
