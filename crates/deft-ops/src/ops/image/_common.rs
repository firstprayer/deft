//! Helpers shared by the image operations.

use deft_core::prelude::*;
use deft_tools_image::image::DynamicImage;

/// HEIC and AVIF are read through ffmpeg; everything else is built in.
pub const IN: &[Format] = &[
    Format::Png,
    Format::Jpeg,
    Format::Webp,
    Format::Gif,
    Format::Tiff,
    Format::Bmp,
    Format::Ico,
    Format::Svg,
    Format::Heic,
    Format::Avif,
];
pub const INOUT: &[Format] = &[
    Format::Png,
    Format::Jpeg,
    Format::Webp,
    Format::Gif,
    Format::Tiff,
    Format::Bmp,
    Format::Ico,
    Format::Avif,
];
pub const QUALITY: ParamSpec = param(
    "quality",
    Kind::Int { min: 1, max: 100 },
    "Quality for JPEG and AVIF",
)
.default("85");

pub const FUZZ: ParamSpec = param(
    "fuzz",
    Kind::Float {
        min: 0.0,
        max: 100.0,
    },
    "Colour tolerance in percent",
);

pub fn open(job: &Job) -> Result<(DynamicImage, Format)> {
    let input = job.input();
    Ok((decode(job, input)?, input.require_format()?))
}

/// Decode any input of a job.
pub fn decode(job: &Job, input: &InputFile) -> Result<DynamicImage> {
    let format = input.require_format()?;
    let result = if matches!(format, Format::Heic | Format::Avif) {
        through_ffmpeg(job, input)
    } else {
        deft_tools_image::decode(input.bytes()?, format, job.ctx.limits.max_pixels)
    };
    result.map_err(|mut e| {
        e.message = format!("{}: {}", input.display, e.message);
        e
    })
}

/// HEIC needs an HEVC decoder, which has no pure-Rust implementation, and
/// ffmpeg 7 or later reads both formats.
fn through_ffmpeg(job: &Job, input: &InputFile) -> Result<DynamicImage> {
    let ffmpeg = deft_engines::require(job.ctx, "ffmpeg", "image decoding")?;
    let out = job.ctx.temp_path("decoded.png");
    let src = input.local_path(job.ctx)?;
    let args: Vec<String> = ["-hide_banner", "-nostdin", "-loglevel", "error", "-y", "-i"]
        .iter()
        .map(|s| s.to_string())
        .chain([src.display().to_string()])
        .chain(
            ["-frames:v", "1", "-update", "1"]
                .iter()
                .map(|s| s.to_string()),
        )
        .chain([out.display().to_string()])
        .collect();
    deft_engines::run(job.ctx, &ffmpeg, &args).map_err(|mut e| {
        e.hint = Some("HEIC and AVIF need ffmpeg 7 or later".into());
        e
    })?;
    deft_tools_image::decode(
        &std::fs::read(&out)?,
        Format::Png,
        job.ctx.limits.max_pixels,
    )
}

/// Write in the input's format; formats deft cannot write fall back to PNG.
pub fn save(job: &Job, img: &DynamicImage, format: Format) -> Result<OpResult> {
    let format = job.target(format);
    let format = if deft_tools_image::WRITABLE.contains(&format) {
        format
    } else {
        Format::Png
    };
    let quality = job.params.opt_int("quality").unwrap_or(85) as u8;
    let bytes = deft_tools_image::encode(
        img,
        format,
        deft_tools_image::Encode {
            quality,
            ..Default::default()
        },
    )?;
    let (w, h) = (img.width(), img.height());
    Ok(OpResult::one(Produced::bytes(bytes, format))
        .fact("width", w)
        .fact("height", h)
        .note(format!("{w}x{h}")))
}
