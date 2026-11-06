use super::_common::{PASSWORD, open, plural, resolve_set, text_image, write};
use deft_core::prelude::*;
use deft_tools_pdf::{Mark, PdfImage, Stamper};

pub static OP: Op = Op {
    id: "pdf.watermark",
    summary: "Stamp text or an image on pages",
    details: &[
        "Any script works. Latin text stays selectable; other scripts are drawn as an image.",
    ],
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::one(&[Format::Pdf], "pdf"),
    params: &[
        param("text", Kind::Text, "Watermark text"),
        param(
            "image",
            Kind::Path,
            "Watermark image (PNG with transparency works best)",
        ),
        param(
            "opacity",
            Kind::Int { min: 1, max: 100 },
            "Opacity in percent",
        )
        .default("20"),
        param(
            "angle",
            Kind::Float {
                min: -180.0,
                max: 180.0,
            },
            "Rotation in degrees, counter-clockwise",
        )
        .default("45"),
        param("position", Kind::Position, "Where on the page").default("center"),
        param("color", Kind::Color, "Text color").default("gray"),
        param(
            "font-size",
            Kind::Float {
                min: 1.0,
                max: 1000.0,
            },
            "Text size in points (default: fits the page)",
        ),
        param("font", Kind::Text, "Font family for non-Latin text"),
        param(
            "scale",
            Kind::Float {
                min: 0.01,
                max: 1.0,
            },
            "Image width as a fraction of the page width",
        )
        .default("0.5"),
        param("pages", Kind::Pages, "Pages to stamp").default("all"),
        PASSWORD,
    ],
    examples: &[
        "deft pdf watermark three-pages.pdf --text DRAFT -o draft.pdf",
        "deft pdf watermark report.pdf --text SECRET --color red -o secret.pdf",
        "deft pdf watermark report.pdf --image photo-1.jpg --opacity 10 --angle 0 -o branded.pdf",
    ],
    tags: &["stamp", "confidential", "draft", "overlay", "logo"],
    result: &["pages"],
    errors: &[ErrorCode::BadValue],
    suffix: Some("watermarked"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let mut doc = open(job.input(), p)?;
    let ids = deft_tools_pdf::page_ids(&doc);
    let indexes = resolve_set(&p.pages("pages"), ids.len())?;
    let opacity = (p.int("opacity") as f64 * 2.55).round() as u8;
    let angle = p.float("angle");
    let pos = p.position("position");
    let mut stamper = Stamper::new();
    enum Source {
        Text(Vec<u8>),
        Image(String, f64),
        /// Non-Latin text drawn as an image: name, then width and height per unit of size.
        Drawn(String, f64, f64),
    }
    let source = match (p.opt_text("text"), p.opt_path("image")) {
        (Some(_), Some(_)) => {
            return Err(err(ErrorCode::BadOption, "use --text or --image, not both"));
        }
        (None, None) => return Err(err(ErrorCode::BadOption, "--text or --image is required")),
        (Some(t), None) => match deft_tools_pdf::win_ansi(&t) {
            Ok(bytes) => Source::Text(bytes),
            Err(_) => {
                let (img, w, h) = text_image(&t, p.color("color"), p.opt_text("font"))?;
                Source::Drawn(stamper.add_image(&mut doc, &img), w, h)
            }
        },
        (None, Some(path)) => {
            let bytes = std::fs::read(&path).map_err(|e| {
                err(
                    ErrorCode::InputNotFound,
                    format!("--image {}: {e}", path.display()),
                )
            })?;
            let format = deft_core::format::sniff(&bytes[..bytes.len().min(4096)], Some(&path))
                .ok_or_else(|| err(ErrorCode::InputUnsupported, "--image: unknown file type"))?;
            let img = PdfImage::from_bytes(&bytes, format, job.ctx.limits.max_pixels)?;
            let (w, h) = img.size();
            Source::Image(stamper.add_image(&mut doc, &img), h as f64 / w as f64)
        }
    };
    for &i in &indexes {
        let page = ids[i];
        let (pw, ph) = deft_tools_pdf::display_size(&doc, page);
        let (s, c) = angle.to_radians().sin_cos();
        // Anchor a box of size (bw, bh), rotated about its centre, inside the page.
        let place = |bw: f64, bh: f64| -> (f64, f64) {
            let (ew, eh) = (
                (bw * c).abs() + (bh * s).abs(),
                (bw * s).abs() + (bh * c).abs(),
            );
            // Keep a margin from the edges for off-centre positions.
            let m = 0.04 * pw.min(ph);
            let cx = m + ew / 2.0 + (pw - ew - 2.0 * m) * pos.x;
            let cy = m + eh / 2.0 + (ph - eh - 2.0 * m) * (1.0 - pos.y);
            (
                cx - (bw / 2.0 * c - bh / 2.0 * s),
                cy - (bw / 2.0 * s + bh / 2.0 * c),
            )
        };
        let mark = match &source {
            Source::Text(bytes) => {
                let unit = deft_tools_pdf::text_width(bytes, 1.0).max(0.1);
                // Default size: span about 70% of the page along the angle.
                let span = (pw / c.abs().max(0.01)).min(ph / s.abs().max(0.01)) * 0.7;
                let size = p
                    .opt_float("font-size")
                    .unwrap_or((span / unit).min(ph * 0.5));
                let (x, y) = place(unit * size, size * 0.72);
                Mark::Text {
                    bytes: bytes.clone(),
                    size,
                    color: p.color("color"),
                    x,
                    y,
                    angle,
                }
            }
            Source::Drawn(name, uw, uh) => {
                let span = (pw / c.abs().max(0.01)).min(ph / s.abs().max(0.01)) * 0.7;
                let size = p
                    .opt_float("font-size")
                    .unwrap_or((span / uw).min(ph * 0.5));
                let (w, h) = (uw * size, uh * size);
                let (x, y) = place(w, h);
                Mark::Image {
                    name: name.clone(),
                    x,
                    y,
                    w,
                    h,
                    angle,
                }
            }
            Source::Image(name, ratio) => {
                let w = pw * p.float("scale");
                let h = w * ratio;
                let (x, y) = place(w, h);
                Mark::Image {
                    name: name.clone(),
                    x,
                    y,
                    w,
                    h,
                    angle,
                }
            }
        };
        stamper.stamp(&mut doc, page, &[mark], opacity)?;
    }
    Ok(OpResult::one(write(&mut doc)?)
        .fact("pages", indexes.len())
        .note(format!("{} stamped", plural(indexes.len(), "page"))))
}
