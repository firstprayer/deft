use super::_common::{
    FFMPEG, VIDEO, ffmpeg, file_result, input_path, probe, stream, strs, temp_out, video_codec_args,
};
use deft_core::format::sniff;
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "video.watermark",
    summary: "Add text or a logo: --text, --image, --position",
    details: &["Any script works; text uses the fonts on this machine."],
    input: Input::each(VIDEO, "video"),
    output: Output::one(VIDEO, "video"),
    params: &[
        param("text", Kind::Text, "Text to add"),
        param("image", Kind::Path, "Logo or picture to add"),
        param(
            "opacity",
            Kind::Int { min: 1, max: 100 },
            "Opacity in percent",
        )
        .default("70"),
        param("position", Kind::Position, "Where on the frame").default("bottom-right"),
        param("color", Kind::Color, "Text color").default("white"),
        param(
            "font-size",
            Kind::Float {
                min: 1.0,
                max: 1000.0,
            },
            "Text size in pixels (default: 5% of the height)",
        ),
        param("font", Kind::Text, "Font family"),
        param(
            "scale",
            Kind::Float {
                min: 0.01,
                max: 1.0,
            },
            "Logo width as a fraction of the frame width",
        )
        .default("0.2"),
    ],
    engines: FFMPEG,
    examples: &[
        "deft video watermark clip.mp4 --text Sample -o marked.mp4",
        "deft video watermark clip.mp4 --image logo.png --position top-left -o branded.mp4",
    ],
    tags: &[
        "logo",
        "brand",
        "overlay",
        "text on video",
        "copyright",
        "stamp",
    ],
    suffix: Some("watermarked"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let format = job.target(job.input().require_format()?);
    let info = probe(job, job.input(), OP.id)?;
    let v = stream(&info, "video")
        .ok_or_else(|| err(ErrorCode::InputUnsupported, "no video stream"))?;
    let (w, h) = (
        v["width"].as_u64().unwrap_or(1280) as u32,
        v["height"].as_u64().unwrap_or(720) as u32,
    );
    let mark = match (p.opt_text("text"), p.opt_path("image")) {
        (Some(_), Some(_)) => {
            return Err(err(ErrorCode::BadOption, "use --text or --image, not both"));
        }
        (None, None) => return Err(err(ErrorCode::BadOption, "--text or --image is required")),
        (Some(t), None) => {
            let size = p.opt_float("font-size").unwrap_or(h as f64 * 0.05).max(8.0) as f32;
            let style = deft_tools_image::TextStyle {
                outline: Some(([0, 0, 0, 255], size * 0.06)),
                font: p.opt_text("font"),
                ..deft_tools_image::TextStyle::new(size, p.color("color"))
            };
            deft_tools_image::render_text(&t, &style)?
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
            let logo = deft_tools_image::decode(&bytes, f, job.ctx.limits.max_pixels)?;
            let target = ((w as f64 * p.float("scale")).round() as u32).max(1);
            let spec = SizeSpec::parse(&format!("{target}x")).map_err(|m| bad_value("scale", m))?;
            deft_tools_image::resize(&logo, &spec, deft_tools_image::Fit::Contain, true)?.to_rgba8()
        }
    };
    let png = job.ctx.temp_path("mark.png");
    let bytes = deft_tools_image::encode(
        &deft_tools_image::image::DynamicImage::ImageRgba8(mark.clone()),
        Format::Png,
        Default::default(),
    )?;
    std::fs::write(&png, bytes)?;
    let (x, y) = deft_tools_image::anchor(
        (w, h),
        mark.dimensions(),
        p.position("position"),
        (w.min(h) as f64 * 0.03) as u32,
    );
    let alpha = p.int("opacity") as f64 / 100.0;
    let out = temp_out(job, format);
    let mut args = strs(&["-i"]);
    args.push(input_path(job)?);
    args.extend(["-i".to_string(), png.display().to_string()]);
    args.extend([
        "-filter_complex".to_string(),
        format!("[1:v]format=rgba,colorchannelmixer=aa={alpha}[m];[0:v][m]overlay={x}:{y}"),
    ]);
    args.extend(video_codec_args(format, None));
    args.push(out.display().to_string());
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, format))
}
