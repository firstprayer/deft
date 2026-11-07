use super::_common::{IN, INOUT, QUALITY, open, save};
use deft_core::format::sniff;
use deft_core::prelude::*;
use deft_tools_image::image::{DynamicImage, RgbaImage};

pub static OP: Op = Op {
    id: "image.watermark",
    summary: "Add text or a logo: --text, --image, --position",
    details: &["Any script works; text uses the fonts on this machine."],
    input: Input::each(IN, "image"),
    output: Output::one(INOUT, "image"),
    params: &[
        param("text", Kind::Text, "Text to add"),
        param("image", Kind::Path, "Logo or picture to add"),
        param(
            "opacity",
            Kind::Int { min: 1, max: 100 },
            "Opacity in percent",
        )
        .default("70"),
        param(
            "angle",
            Kind::Float {
                min: -180.0,
                max: 180.0,
            },
            "Rotation in degrees, counter-clockwise",
        )
        .default("0"),
        param("position", Kind::Position, "Where on the image").default("bottom-right"),
        param("color", Kind::Color, "Text color").default("white"),
        param(
            "font-size",
            Kind::Float {
                min: 1.0,
                max: 1000.0,
            },
            "Text size in pixels (default: 5% of the image height)",
        ),
        param("font", Kind::Text, "Font family"),
        param(
            "scale",
            Kind::Float {
                min: 0.01,
                max: 1.0,
            },
            "Logo width as a fraction of the image width",
        )
        .default("0.2"),
        QUALITY,
    ],
    examples: &[
        "deft image watermark photo-1.jpg --text Sample -o marked.jpg",
        "deft image watermark photo-2.jpg --text \"© Acme\" --position center -o centered.jpg",
        "deft image watermark photo-3.jpg --image logo.png --position top-left -o branded.jpg",
    ],
    tags: &[
        "caption",
        "text on image",
        "copyright",
        "logo",
        "stamp",
        "overlay",
        "label",
    ],
    errors: &[ErrorCode::InputUnsupported],
    result: &["width", "height"],
    suffix: Some("watermarked"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let (img, format) = open(job)?;
    let (w, h) = (img.width(), img.height());
    let angle = p.float("angle") as f32;
    let margin = (w.min(h) as f64 * 0.03).round() as u32;
    let room = (w - 2 * margin, h - 2 * margin);
    let mut warning = None;
    let mark = match (p.opt_text("text"), p.opt_path("image")) {
        (Some(_), Some(_)) => {
            return Err(err(ErrorCode::BadOption, "use --text or --image, not both"));
        }
        (None, None) => return Err(err(ErrorCode::BadOption, "--text or --image is required")),
        (Some(t), None) => {
            let size = p.opt_float("font-size").unwrap_or(h as f64 * 0.05).max(8.0) as f32;
            let outline = Some(([0, 0, 0, 255], size * 0.06));
            let style = deft_tools_image::TextStyle {
                angle,
                outline,
                font: p.opt_text("font"),
                ..deft_tools_image::TextStyle::new(size, p.color("color"))
            };
            let (mark, used) = fit_text(&t, style, room)?;
            if used < 10.0 {
                warning = Some(format!(
                    "the text only fits at {used:.0} pixels high; shorten it or use a larger image"
                ));
            }
            mark
        }
        (None, Some(path)) => {
            let bytes = std::fs::read(&path).map_err(|e| {
                err(
                    ErrorCode::InputNotFound,
                    format!("--image {}: {e}", path.display()),
                )
            })?;
            let f = sniff(&bytes[..bytes.len().min(4096)], Some(&path))
                .ok_or_else(|| err(ErrorCode::InputUnsupported, "--image: unknown file type"))?;
            let target = ((w as f64 * p.float("scale")).round() as u32).max(1);
            let logo = if f == Format::Svg {
                deft_tools_image::render_svg(&bytes, Some(target))?
            } else {
                deft_tools_image::decode(&bytes, f, job.ctx.limits.max_pixels)?
            };
            let spec = SizeSpec::parse(&format!("{target}x")).map_err(|m| bad_value("scale", m))?;
            let sized =
                deft_tools_image::resize(&logo, &spec, deft_tools_image::Fit::Contain, true)?;
            deft_tools_image::rotate_any(&sized.to_rgba8(), angle)
        }
    };
    let mut base = img.to_rgba8();
    let (x, y) =
        deft_tools_image::anchor((w, h), mark.dimensions(), p.position("position"), margin);
    deft_tools_image::overlay(&mut base, &mark, x, y, p.int("opacity") as f32 / 100.0);
    let out = if img.color().has_alpha() {
        DynamicImage::ImageRgba8(base)
    } else {
        DynamicImage::ImageRgb8(DynamicImage::ImageRgba8(base).to_rgb8())
    };
    let res = save(job, &out, format)?;
    Ok(match warning {
        Some(w) => res.warn(w),
        None => res,
    })
}

/// Draw text so that all of it is on the image: wrap it if it runs too wide,
/// then shrink it if it still does not fit. Returns the text height used too.
fn fit_text(
    text: &str,
    style: deft_tools_image::TextStyle,
    room: (u32, u32),
) -> Result<(RgbaImage, f32)> {
    let mark = deft_tools_image::render_text(text, &style)?;
    if mark.width() <= room.0 && mark.height() <= room.1 {
        return Ok((mark, style.size));
    }
    let wrapped = if style.angle == 0.0 {
        deft_tools_image::wrap_text(text, room.0 as f32 / style.size).join("\n")
    } else {
        text.to_string()
    };
    let mark = deft_tools_image::render_text(&wrapped, &style)?;
    let scale = (room.0 as f32 / mark.width() as f32)
        .min(room.1 as f32 / mark.height() as f32)
        .min(1.0);
    if scale == 1.0 {
        return Ok((mark, style.size));
    }
    // Slightly under, because the outline does not shrink in step.
    let size = style.size * scale * 0.98;
    let style = deft_tools_image::TextStyle {
        size,
        outline: style.outline.map(|(c, w)| (c, w * scale)),
        ..style
    };
    Ok((deft_tools_image::render_text(&wrapped, &style)?, size))
}
