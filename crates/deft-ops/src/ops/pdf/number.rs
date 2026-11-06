use super::_common::{PASSWORD, open, plural, text_image, write};
use deft_core::prelude::*;
use deft_tools_pdf::{Mark, Stamper};

pub static OP: Op = Op {
    id: "pdf.number",
    summary: "Add page numbers: --format",
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::one(&[Format::Pdf], "pdf"),
    params: &[
        param("format", Kind::Text, "Template with {n} and {total}").default("{n}"),
        param("position", Kind::Position, "Where on the page").default("bottom"),
        param("margin", Kind::Length, "Distance from the page edge").default("10mm"),
        param(
            "font-size",
            Kind::Float {
                min: 1.0,
                max: 1000.0,
            },
            "Text size in points",
        )
        .default("10"),
        param("color", Kind::Color, "Text color").default("black"),
        param(
            "first-page",
            Kind::Int {
                min: 1,
                max: 1_000_000,
            },
            "First page that gets a number",
        )
        .default("1"),
        param(
            "start",
            Kind::Int {
                min: 0,
                max: 1_000_000,
            },
            "Number shown on that page",
        )
        .default("1"),
        PASSWORD,
    ],
    examples: &[
        "deft pdf number three-pages.pdf -o numbered.pdf",
        "deft pdf number report.pdf --format \"Page {n} of {total}\" --first-page 2",
    ],
    tags: &["page numbers", "paginate", "footer"],
    result: &["numbered"],
    suffix: Some("numbered"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let mut doc = open(job.input(), p)?;
    let ids = deft_tools_pdf::page_ids(&doc);
    let first = p.int("first-page") as usize - 1;
    let start = p.int("start");
    let total = ids.len().saturating_sub(first) as i64 + start - 1;
    let (size, margin, pos) = (
        p.float("font-size"),
        p.points("margin"),
        p.position("position"),
    );
    let template = p.text("format");
    let mut stamper = Stamper::new();
    let mut numbered = 0;
    for (i, &page) in ids.iter().enumerate().skip(first) {
        let n = start + (i - first) as i64;
        let label = template
            .replace("{n}", &n.to_string())
            .replace("{total}", &total.to_string());
        let (pw, ph) = deft_tools_pdf::display_size(&doc, page);
        let corner = |w: f64, h: f64| {
            (
                margin + (pw - 2.0 * margin - w) * pos.x,
                margin + (ph - 2.0 * margin - h) * (1.0 - pos.y),
            )
        };
        let mark = match deft_tools_pdf::win_ansi(&label) {
            Ok(bytes) => {
                let (x, y) = corner(deft_tools_pdf::text_width(&bytes, size), size);
                Mark::Text {
                    bytes,
                    size,
                    color: p.color("color"),
                    x,
                    y,
                    angle: 0.0,
                }
            }
            Err(_) => {
                let (img, uw, uh) = text_image(&label, p.color("color"), None)?;
                let name = stamper.add_image(&mut doc, &img);
                let (w, h) = (uw * size, uh * size);
                let (x, y) = corner(w, h);
                Mark::Image {
                    name,
                    x,
                    y,
                    w,
                    h,
                    angle: 0.0,
                }
            }
        };
        stamper.stamp(&mut doc, page, &[mark], 255)?;
        numbered += 1;
    }
    Ok(OpResult::one(write(&mut doc)?)
        .fact("numbered", numbered)
        .note(format!("{} numbered", plural(numbered, "page"))))
}
