use super::_common::{PASSWORD, open, plural, resolve_set, write};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "pdf.crop",
    summary: "Trim pages: --margin, --box, --auto",
    details: &[
        "--box is x,y,width,height in points from the top-left of the page.",
        "--auto cuts to the visible content; --margin then keeps that much around it.",
    ],
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::one(&[Format::Pdf], "pdf"),
    params: &[
        param("margin", Kind::Length, "Amount to cut from each side"),
        param("box", Kind::Text, "Region to keep: x,y,width,height"),
        param("auto", Kind::Flag, "Cut to the content"),
        param("pages", Kind::Pages, "Pages to crop").default("all"),
        PASSWORD,
    ],
    examples: &[
        "deft pdf crop report.pdf --margin 10mm -o tight.pdf",
        "deft pdf crop report.pdf --auto --margin 5mm -o fit.pdf",
    ],
    tags: &["trim margins", "cut", "whitespace", "cropbox"],
    result: &["cropped"],
    suffix: Some("cropped"),
    run,
    ..Op::DEFAULT
};

fn region(text: &str) -> Result<[f64; 4]> {
    let v: Vec<f64> = text
        .split(',')
        .map(|s| s.trim().parse())
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| bad_value("box", "use x,y,width,height in points"))?;
    <[f64; 4]>::try_from(v).map_err(|_| bad_value("box", "use x,y,width,height in points"))
}

/// The smallest region holding both, as x, y, width, height.
fn union(a: Option<[f64; 4]>, b: Option<[f64; 4]>) -> Option<[f64; 4]> {
    match (a, b) {
        (Some(a), Some(b)) => {
            let (x0, y0) = (a[0].min(b[0]), a[1].min(b[1]));
            let (x1, y1) = (
                (a[0] + a[2]).max(b[0] + b[2]),
                (a[1] + a[3]).max(b[1] + b[3]),
            );
            Some([x0, y0, x1 - x0, y1 - y0])
        }
        (a, b) => a.or(b),
    }
}

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let mut doc = open(job.input(), p)?;
    let ids = deft_tools_pdf::page_ids(&doc);
    let indexes = resolve_set(&p.pages("pages"), ids.len())?;
    let margin = p.opt_points("margin");
    let area = p.opt_text("box").map(|b| region(&b)).transpose()?;
    if p.flag("auto") {
        let r = deft_tools_pdf::Renderer::of(&doc)?;
        let pad = margin.unwrap_or(0.0);
        for &i in &indexes {
            let drawn = deft_tools_pdf::pages::content_region(&r, i, job.ctx.limits.max_pixels)?;
            let fields = deft_tools_pdf::pages::annotations_region(&doc, ids[i]);
            let Some([x, y, w, h]) = union(drawn, fields) else {
                continue;
            };
            let (pw, ph) = deft_tools_pdf::display_size(&doc, ids[i]);
            let (x0, y0) = ((x - pad).max(0.0), (y - pad).max(0.0));
            let (x1, y1) = ((x + w + pad).min(pw), (y + h + pad).min(ph));
            deft_tools_pdf::pages::crop_region(&mut doc, ids[i], [x0, y0, x1 - x0, y1 - y0])?;
        }
    } else {
        match (area, margin) {
            (Some(_), Some(_)) => {
                return Err(err(ErrorCode::BadOption, "use --margin or --box, not both"));
            }
            (None, None) => {
                return Err(err(ErrorCode::BadOption, "give --margin, --box or --auto"));
            }
            (Some(b), None) => {
                for &i in &indexes {
                    deft_tools_pdf::pages::crop_region(&mut doc, ids[i], b)?;
                }
            }
            (None, Some(m)) => {
                for &i in &indexes {
                    deft_tools_pdf::pages::crop_margin(&mut doc, ids[i], m)?;
                }
            }
        }
    }
    Ok(OpResult::one(write(&mut doc)?)
        .fact("cropped", indexes.len())
        .note(plural(indexes.len(), "page")))
}
