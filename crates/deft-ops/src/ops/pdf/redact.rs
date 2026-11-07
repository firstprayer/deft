use super::_common::{PASSWORD, open, plural, resolve_set};
use deft_core::prelude::*;
use std::collections::HashMap;

pub static OP: Op = Op {
    id: "pdf.redact",
    summary: "Black out for good: --box or --text",
    details: &[
        "--box is in points from the top-left of the page; separate several with semicolons.",
        "--text blacks out each place it appears. Check the result: scans have no text.",
        "Redacted pages become images, so nothing survives under the boxes.",
        "Their form fields go too. --text also clears the word from metadata and bookmarks.",
    ],
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::one(&[Format::Pdf], "pdf"),
    params: &[
        param("box", Kind::Text, "Regions: x,y,width,height;..."),
        param("text", Kind::Text, "Words to remove, ignoring case"),
        param("pages", Kind::Pages, "Pages to redact").default("all"),
        param(
            "dpi",
            Kind::Int { min: 72, max: 600 },
            "Resolution of redacted pages",
        )
        .default("200"),
        PASSWORD,
    ],
    examples: &[
        "deft pdf redact report.pdf --box 50,60,300,40 --pages 1 -o safe.pdf",
        "deft pdf redact report.pdf --text 'page 2' -o clean.pdf",
    ],
    tags: &[
        "censor",
        "black out",
        "hide",
        "remove sensitive",
        "blackout",
    ],
    result: &["redacted"],
    suffix: Some("redacted"),
    run,
    ..Op::DEFAULT
};

fn regions(text: &str) -> Result<Vec<[f64; 4]>> {
    text.split(';')
        .map(|part| {
            let v: Vec<f64> = part
                .split(',')
                .map(|s| s.trim().parse())
                .collect::<std::result::Result<_, _>>()
                .map_err(|_| bad_value("box", "use x,y,width,height in points"))?;
            <[f64; 4]>::try_from(v).map_err(|_| bad_value("box", "use x,y,width,height in points"))
        })
        .collect()
}

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let mut doc = open(job.input(), p)?;
    let fixed = match p.opt_text("box") {
        Some(b) => regions(&b)?,
        None => vec![],
    };
    let words = p.opt_text("text");
    if fixed.is_empty() && words.is_none() {
        return Err(err(ErrorCode::BadOption, "give --box or --text"));
    }
    let ids = deft_tools_pdf::page_ids(&doc);
    let indexes = resolve_set(&p.pages("pages"), ids.len())?;
    let bytes = deft_tools_pdf::save(&mut doc)?;
    let r = deft_tools_pdf::Renderer::new(&bytes)?;
    let mut found: HashMap<usize, Vec<[f64; 4]>> = HashMap::new();
    if let Some(t) = &words {
        r.each_page_text(&indexes, |i, page| {
            let hits = page.find(t).into_iter().flat_map(|hit| hit.regions);
            found.insert(i, hits.collect());
        });
    }
    let mut redacted = 0;
    for &i in &indexes {
        let mut boxes = fixed.clone();
        boxes.extend(found.remove(&i).unwrap_or_default());
        if boxes.is_empty() {
            continue;
        }
        redacted += 1;
        let (w, h) = deft_tools_pdf::display_size(&doc, ids[i]);
        let img = deft_tools_pdf::pages::redacted_image(
            &r,
            i,
            &boxes,
            p.int("dpi") as f64,
            job.ctx.limits.max_pixels,
        )?;
        let pic = deft_tools_pdf::PdfImage::from_image(
            &deft_tools_image::image::DynamicImage::ImageRgb8(img),
        );
        deft_tools_pdf::pages::replace_with_image(&mut doc, ids[i], &pic, w, h)?;
    }
    let mut res = OpResult::one(super::_common::write(&mut doc)?)
        .fact("redacted", redacted)
        .note(plural(redacted, "page"));
    if redacted == 0 {
        res = res.warn("nothing matched, so nothing was redacted");
    }
    Ok(res)
}
