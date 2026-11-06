//! Image capabilities: decode (with EXIF orientation), encode, resize, crop,
//! strip metadata, optimise PNG, render SVG. Pure functions; no CLI knowledge.

use deft_core::error::{ErrorCode, Result, err};
use deft_core::format::Format;
use image::{DynamicImage, GenericImageView, ImageDecoder, ImageReader, RgbImage, RgbaImage};
use std::io::Cursor;

pub mod codes;
pub mod edit;
pub mod exif;

pub use image;

/// Decode any supported image, applying EXIF orientation so the result is
/// upright. `max_pixels` guards against decompression bombs.
pub fn decode(bytes: &[u8], format: Format, max_pixels: u64) -> Result<DynamicImage> {
    match format {
        Format::Svg => return render_svg(bytes, None),
        // Decoded through ffmpeg by the operations, before this point.
        Format::Heic | Format::Avif => {
            return Err(err(
                ErrorCode::InputUnsupported,
                format!("reading {format} needs ffmpeg"),
            ));
        }
        _ => {}
    }
    let reader = ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| err(ErrorCode::InputCorrupt, e.to_string()))?;
    let mut decoder = reader.into_decoder().map_err(image_err)?;
    let (w, h) = decoder.dimensions();
    if w as u64 * h as u64 > max_pixels {
        return Err(err(
            ErrorCode::LimitExceeded,
            format!("image is {w}x{h}, over the {max_pixels}-pixel safety limit"),
        ));
    }
    let orientation = decoder
        .orientation()
        .unwrap_or(image::metadata::Orientation::NoTransforms);
    let mut img = DynamicImage::from_decoder(decoder).map_err(image_err)?;
    img.apply_orientation(orientation);
    Ok(img)
}

fn image_err(e: image::ImageError) -> deft_core::error::DeftError {
    match e {
        image::ImageError::Unsupported(u) => err(ErrorCode::InputUnsupported, u.to_string()),
        image::ImageError::Limits(l) => err(ErrorCode::LimitExceeded, l.to_string()),
        other => err(
            ErrorCode::InputCorrupt,
            other.to_string().trim().to_string(),
        ),
    }
}

/// Image dimensions without decoding pixels.
pub fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?
        .into_dimensions()
        .ok()
}

/// EXIF orientation value (1..=8) of an image, 1 when absent.
pub fn exif_orientation(bytes: &[u8]) -> u8 {
    ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()
        .and_then(|r| r.into_decoder().ok())
        .and_then(|mut d| d.orientation().ok())
        .map(|o| o.to_exif())
        .unwrap_or(1)
}

/// Width, height and component count from a JPEG's SOF marker.
pub fn jpeg_info(bytes: &[u8]) -> Option<(u32, u32, u8)> {
    if !bytes.starts_with(&[0xFF, 0xD8]) {
        return None;
    }
    let mut i = 2;
    while i + 9 < bytes.len() {
        if bytes[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = bytes[i + 1];
        if marker == 0xD8 || marker == 0x01 || (0xD0..=0xD7).contains(&marker) || marker == 0xFF {
            i += if marker == 0xFF { 1 } else { 2 };
            continue;
        }
        let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        let is_sof = matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC);
        if is_sof {
            let h = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
            let w = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32;
            return Some((w, h, bytes[i + 9]));
        }
        i += 2 + len;
    }
    None
}

/// Encoding options.
#[derive(Clone, Copy, Debug)]
pub struct Encode {
    /// 1-100, for lossy formats.
    pub quality: u8,
    /// Background used when a format without alpha receives transparent pixels.
    pub background: [u8; 3],
}

impl Default for Encode {
    fn default() -> Self {
        Encode {
            quality: 85,
            background: [255, 255, 255],
        }
    }
}

pub const WRITABLE: &[Format] = &[
    Format::Png,
    Format::Jpeg,
    Format::Webp,
    Format::Gif,
    Format::Tiff,
    Format::Bmp,
    Format::Ico,
    Format::Avif,
];
pub const READABLE: &[Format] = &[
    Format::Png,
    Format::Jpeg,
    Format::Webp,
    Format::Gif,
    Format::Tiff,
    Format::Bmp,
    Format::Ico,
    Format::Svg,
];

/// Paint transparent pixels over a solid background.
pub fn flatten(img: &DynamicImage, bg: [u8; 3]) -> RgbImage {
    if !img.color().has_alpha() {
        return img.to_rgb8();
    }
    let rgba = img.to_rgba8();
    let mut out = RgbImage::new(rgba.width(), rgba.height());
    for (o, p) in out.pixels_mut().zip(rgba.pixels()) {
        let a = p[3] as u32;
        for c in 0..3 {
            o[c] = ((p[c] as u32 * a + bg[c] as u32 * (255 - a) + 127) / 255) as u8;
        }
    }
    out
}

pub fn has_transparency(img: &DynamicImage) -> bool {
    img.color().has_alpha() && img.to_rgba8().pixels().any(|p| p[3] < 255)
}

/// Encode an image in the given format.
pub fn encode(img: &DynamicImage, format: Format, opt: Encode) -> Result<Vec<u8>> {
    use image::ImageEncoder;
    let mut out = Vec::new();
    let enc_err =
        |e: image::ImageError| err(ErrorCode::Internal, format!("encoding {format}: {e}"));
    match format {
        Format::Jpeg => {
            let q = opt.quality.clamp(1, 100);
            let mut enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, q);
            if img.color().has_color() {
                enc.encode_image(&flatten(img, opt.background))
                    .map_err(enc_err)?;
            } else {
                let gray = DynamicImage::ImageRgb8(flatten(img, opt.background)).to_luma8();
                enc.encode_image(&gray).map_err(enc_err)?;
            }
        }
        Format::Png => {
            let enc = image::codecs::png::PngEncoder::new_with_quality(
                &mut out,
                image::codecs::png::CompressionType::Best,
                image::codecs::png::FilterType::Adaptive,
            );
            let img = normalize_depth(img);
            enc.write_image(
                img.as_bytes(),
                img.width(),
                img.height(),
                img.color().into(),
            )
            .map_err(enc_err)?;
        }
        Format::Webp => {
            let enc = image::codecs::webp::WebPEncoder::new_lossless(&mut out);
            let px = if img.color().has_alpha() {
                DynamicImage::ImageRgba8(img.to_rgba8())
            } else {
                DynamicImage::ImageRgb8(img.to_rgb8())
            };
            enc.write_image(px.as_bytes(), px.width(), px.height(), px.color().into())
                .map_err(enc_err)?;
        }
        Format::Avif => {
            let enc = image::codecs::avif::AvifEncoder::new_with_speed_quality(
                &mut out,
                7,
                opt.quality.clamp(1, 100),
            );
            let rgba = img.to_rgba8();
            enc.write_image(
                rgba.as_raw(),
                rgba.width(),
                rgba.height(),
                image::ExtendedColorType::Rgba8,
            )
            .map_err(enc_err)?;
        }
        Format::Gif => {
            let mut enc = image::codecs::gif::GifEncoder::new(&mut out);
            enc.encode_frame(image::Frame::new(img.to_rgba8()))
                .map_err(enc_err)?;
        }
        Format::Ico => {
            let img = if img.width() > 256 || img.height() > 256 {
                img.resize(256, 256, image::imageops::FilterType::Lanczos3)
            } else {
                img.clone()
            };
            let rgba = img.to_rgba8();
            image::codecs::ico::IcoEncoder::new(&mut out)
                .write_image(
                    rgba.as_raw(),
                    rgba.width(),
                    rgba.height(),
                    image::ExtendedColorType::Rgba8,
                )
                .map_err(enc_err)?;
        }
        Format::Tiff => {
            normalize_depth(img)
                .write_to(&mut Cursor::new(&mut out), image::ImageFormat::Tiff)
                .map_err(enc_err)?;
        }
        Format::Bmp => {
            let px = if img.color().has_alpha() {
                DynamicImage::ImageRgba8(img.to_rgba8())
            } else {
                DynamicImage::ImageRgb8(img.to_rgb8())
            };
            px.write_to(&mut Cursor::new(&mut out), image::ImageFormat::Bmp)
                .map_err(enc_err)?;
        }
        other => {
            return Err(err(
                ErrorCode::InputUnsupported,
                format!("cannot write {other} images"),
            ));
        }
    }
    Ok(out)
}

/// 16-bit and float images become 8-bit for formats that expect it.
fn normalize_depth(img: &DynamicImage) -> DynamicImage {
    use image::ColorType::*;
    match img.color() {
        L8 | La8 | Rgb8 | Rgba8 => img.clone(),
        L16 => DynamicImage::ImageLuma8(img.to_luma8()),
        La16 => DynamicImage::ImageLumaA8(img.to_luma_alpha8()),
        c if c.has_alpha() => DynamicImage::ImageRgba8(img.to_rgba8()),
        _ => DynamicImage::ImageRgb8(img.to_rgb8()),
    }
}

/// High-quality (Lanczos3, SIMD) resize to exact dimensions.
pub fn resize_exact(img: &DynamicImage, w: u32, h: u32) -> Result<DynamicImage> {
    use fast_image_resize::{
        FilterType, IntoImageView, ResizeAlg, ResizeOptions, Resizer, images::Image,
    };
    if (w, h) == img.dimensions() {
        return Ok(img.clone());
    }
    let src = normalize_depth(img);
    let pixel_type = src
        .pixel_type()
        .ok_or_else(|| err(ErrorCode::InputUnsupported, "unsupported pixel format"))?;
    let mut dst = Image::new(w, h, pixel_type);
    Resizer::new()
        .resize(
            &src,
            &mut dst,
            &ResizeOptions::new().resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3)),
        )
        .map_err(|e| err(ErrorCode::Internal, format!("resize: {e}")))?;
    let buf = dst.into_vec();
    let out = match src.color() {
        image::ColorType::L8 => image::GrayImage::from_raw(w, h, buf).map(DynamicImage::ImageLuma8),
        image::ColorType::La8 => {
            image::GrayAlphaImage::from_raw(w, h, buf).map(DynamicImage::ImageLumaA8)
        }
        image::ColorType::Rgb8 => RgbImage::from_raw(w, h, buf).map(DynamicImage::ImageRgb8),
        _ => RgbaImage::from_raw(w, h, buf).map(DynamicImage::ImageRgba8),
    };
    out.ok_or_else(|| err(ErrorCode::Internal, "resize produced a bad buffer"))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fit {
    /// Fit inside the box, keeping the aspect ratio.
    Contain,
    /// Fill the box, keeping the aspect ratio, cropping the overflow.
    Cover,
    /// Stretch to the box exactly.
    Fill,
}

impl Fit {
    pub fn parse(s: &str) -> Fit {
        match s {
            "cover" => Fit::Cover,
            "fill" => Fit::Fill,
            _ => Fit::Contain,
        }
    }
}

/// Resize according to a size spec. Without `upscale`, images never grow.
pub fn resize(
    img: &DynamicImage,
    spec: &deft_core::value::SizeSpec,
    fit: Fit,
    upscale: bool,
) -> Result<DynamicImage> {
    let (w, h) = img.dimensions();
    let (tw, th) = spec.target(w, h);
    let (mut rw, mut rh) = if spec.is_box() {
        match fit {
            Fit::Fill => (tw, th),
            Fit::Contain | Fit::Cover => {
                let sx = tw as f64 / w as f64;
                let sy = th as f64 / h as f64;
                let s = if fit == Fit::Contain {
                    sx.min(sy)
                } else {
                    sx.max(sy)
                };
                (
                    ((w as f64 * s).round() as u32).max(1),
                    ((h as f64 * s).round() as u32).max(1),
                )
            }
        }
    } else {
        (tw, th)
    };
    if !upscale && fit != Fit::Fill && (rw > w || rh > h) {
        let s = (w as f64 / rw as f64).min(h as f64 / rh as f64).min(1.0);
        rw = ((rw as f64 * s).round() as u32).max(1);
        rh = ((rh as f64 * s).round() as u32).max(1);
    }
    let resized = resize_exact(img, rw, rh)?;
    if spec.is_box() && fit == Fit::Cover && (rw > tw || rh > th) {
        let cw = tw.min(rw);
        let ch = th.min(rh);
        return Ok(resized.crop_imm((rw - cw) / 2, (rh - ch) / 2, cw, ch));
    }
    Ok(resized)
}

/// Crop rectangle for an aspect ratio, anchored at a nine-grid position.
pub fn aspect_box(
    w: u32,
    h: u32,
    aw: f64,
    ah: f64,
    anchor: deft_core::value::Position,
) -> (u32, u32, u32, u32) {
    let target = aw / ah;
    let (cw, ch) = if w as f64 / h as f64 > target {
        (((h as f64) * target).round() as u32, h)
    } else {
        (w, ((w as f64) / target).round() as u32)
    };
    let (cw, ch) = (cw.clamp(1, w), ch.clamp(1, h));
    let x = ((w - cw) as f64 * anchor.x).round() as u32;
    let y = ((h - ch) as f64 * anchor.y).round() as u32;
    (x, y, cw, ch)
}

/// The fonts compiled into deft. Everything goes through this one function:
/// calling `typst_assets::fonts` from two crates links a second 9 MB copy.
#[inline(never)]
pub fn bundled_fonts() -> Vec<&'static [u8]> {
    typst_assets::fonts().collect()
}

/// System fonts plus the bundled ones, loaded once per process.
fn font_database() -> std::sync::Arc<resvg::usvg::fontdb::Database> {
    static DB: std::sync::OnceLock<std::sync::Arc<resvg::usvg::fontdb::Database>> =
        std::sync::OnceLock::new();
    DB.get_or_init(|| {
        let mut db = resvg::usvg::fontdb::Database::new();
        db.load_system_fonts();
        for font in bundled_fonts() {
            db.load_font_data(font.to_vec());
        }
        std::sync::Arc::new(db)
    })
    .clone()
}

/// How to draw text into an image.
#[derive(Clone, Debug)]
pub struct TextStyle {
    /// Text height in pixels.
    pub size: f32,
    pub color: [u8; 4],
    /// Degrees, counter-clockwise.
    pub angle: f32,
    /// Outline colour and width in pixels, for legibility on video.
    pub outline: Option<([u8; 4], f32)>,
    pub font: Option<String>,
}

impl TextStyle {
    pub fn new(size: f32, color: [u8; 4]) -> TextStyle {
        TextStyle {
            size,
            color,
            angle: 0.0,
            outline: None,
            font: None,
        }
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn css_color(c: [u8; 4]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// Rough width of text, in ems, for sizing a canvas and wrapping lines.
fn width_em(text: &str) -> f32 {
    text.chars()
        .map(|c| if (c as u32) >= 0x2E80 { 1.05 } else { 0.62 })
        .sum()
}

/// Wrap text to at most `max_em` wide. Breaks at spaces, or anywhere in CJK.
pub fn wrap_text(text: &str, max_em: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        let mut line = String::new();
        let mut word = String::new();
        let flush = |line: &mut String, word: &mut String, lines: &mut Vec<String>| {
            if width_em(line) + width_em(word) > max_em && !line.is_empty() {
                lines.push(line.trim_end().to_string());
                line.clear();
            }
            line.push_str(word);
            word.clear();
        };
        for c in paragraph.chars() {
            word.push(c);
            if c == ' ' || (c as u32) >= 0x2E80 {
                flush(&mut line, &mut word, &mut lines);
            }
        }
        flush(&mut line, &mut word, &mut lines);
        if !line.trim().is_empty() {
            lines.push(line.trim_end().to_string());
        }
    }
    lines
}

/// Draw text on a transparent canvas, trimmed to its ink. Works for any script
/// the machine has a font for.
pub fn render_text(text: &str, style: &TextStyle) -> Result<RgbaImage> {
    let lines: Vec<&str> = text.split('\n').collect();
    let size = style.size;
    let line_h = size * 1.3;
    let widest = lines.iter().map(|l| width_em(l)).fold(1.0, f32::max) * size;
    let tall = line_h * lines.len() as f32;
    let side = (widest * widest + tall * tall).sqrt() + size * 2.0;
    let (cw, ch) = if style.angle == 0.0 {
        (widest + size * 2.0, tall + size)
    } else {
        (side, side)
    };
    if cw * ch > 200_000_000.0 {
        return Err(err(
            ErrorCode::LimitExceeded,
            "the text is too large to draw",
        ));
    }
    let family = style
        .font
        .as_deref()
        .map(|f| format!("'{}', ", xml_escape(f)))
        .unwrap_or_default();
    let stack = format!(
        "{family}'PingFang SC', 'Hiragino Sans GB', 'Noto Sans CJK SC', 'Microsoft YaHei', 'Helvetica Neue', Arial, 'DejaVu Sans', 'DejaVu Sans Mono', sans-serif"
    );
    let outline = style
        .outline
        .map(|(c, w)| format!(" stroke=\"{}\" stroke-width=\"{w}\" stroke-linejoin=\"round\" paint-order=\"stroke\"", css_color(c)))
        .unwrap_or_default();
    let mut body = String::new();
    let top = (ch - tall) / 2.0 + size;
    for (i, line) in lines.iter().enumerate() {
        body.push_str(&format!(
            "<text x=\"{}\" y=\"{}\" text-anchor=\"middle\">{}</text>",
            cw / 2.0,
            top + i as f32 * line_h,
            xml_escape(line)
        ));
    }
    let opacity = style.color[3] as f32 / 255.0;
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{cw}\" height=\"{ch}\"><g font-family=\"{stack}\" font-size=\"{size}\" fill=\"{}\" fill-opacity=\"{opacity}\"{outline} transform=\"rotate({} {} {})\" xml:space=\"preserve\">{body}</g></svg>",
        css_color(style.color),
        -style.angle,
        cw / 2.0,
        ch / 2.0,
    );
    let opt = resvg::usvg::Options {
        fontdb: font_database(),
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_data(svg.as_bytes(), &opt)
        .map_err(|e| err(ErrorCode::Internal, format!("text: {e}")))?;
    let (pw, ph) = (cw.ceil() as u32, ch.ceil() as u32);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(pw, ph)
        .ok_or_else(|| err(ErrorCode::LimitExceeded, "the text is too large to draw"))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::identity(),
        &mut pixmap.as_mut(),
    );
    let mut data = pixmap.take();
    unpremultiply(&mut data);
    let img =
        RgbaImage::from_raw(pw, ph, data).ok_or_else(|| err(ErrorCode::Internal, "text buffer"))?;
    trim_transparent(&img).ok_or_else(|| {
        err(
            ErrorCode::InputUnsupported,
            "no font on this machine can draw this text",
        )
        .hint("install a font for this script, or pass --image")
    })
}

fn unpremultiply(data: &mut [u8]) {
    for px in data.as_chunks_mut::<4>().0 {
        let a = px[3] as u32;
        if a > 0 && a < 255 {
            for v in &mut px[..3] {
                *v = ((*v as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
}

/// Crop to the non-transparent area; None when fully transparent.
fn trim_transparent(img: &RgbaImage) -> Option<RgbaImage> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for (x, y, p) in img.enumerate_pixels() {
        if p[3] > 0 {
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
    }
    (x0 != u32::MAX)
        .then(|| image::imageops::crop_imm(img, x0, y0, x1 - x0 + 1, y1 - y0 + 1).to_image())
}

/// The colour between pixels, so turned edges stay smooth.
fn bilinear(img: &RgbaImage, x: f32, y: f32) -> image::Rgba<u8> {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let at = |dx: i64, dy: i64| {
        let px = (x0 as i64 + dx).clamp(0, img.width() as i64 - 1) as u32;
        let py = (y0 as i64 + dy).clamp(0, img.height() as i64 - 1) as u32;
        img.get_pixel(px, py).0
    };
    let (a, b, c, d) = (at(0, 0), at(1, 0), at(0, 1), at(1, 1));
    let mix = |n: usize| {
        let top = a[n] as f32 * (1.0 - fx) + b[n] as f32 * fx;
        let low = c[n] as f32 * (1.0 - fx) + d[n] as f32 * fx;
        (top * (1.0 - fy) + low * fy).round() as u8
    };
    image::Rgba([mix(0), mix(1), mix(2), mix(3)])
}

/// Rotate by any angle (degrees, counter-clockwise) onto a larger transparent canvas.
pub fn rotate_any(img: &RgbaImage, degrees: f32) -> RgbaImage {
    if degrees == 0.0 {
        return img.clone();
    }
    let (s, c) = degrees.to_radians().sin_cos();
    let (w, h) = (img.width() as f32, img.height() as f32);
    // The small subtraction keeps float error at 90 degrees from adding a pixel.
    let (nw, nh) = (
        (w * c.abs() + h * s.abs() - 0.001).ceil(),
        (w * s.abs() + h * c.abs() - 0.001).ceil(),
    );
    let mut out = RgbaImage::new(nw as u32, nh as u32);
    let (cx, cy, ncx, ncy) = (w / 2.0, h / 2.0, nw / 2.0, nh / 2.0);
    for (x, y, p) in out.enumerate_pixels_mut() {
        let (dx, dy) = (x as f32 + 0.5 - ncx, y as f32 + 0.5 - ncy);
        // Image y points down, so a counter-clockwise turn inverts like this.
        let (sx, sy) = (c * dx - s * dy + cx, s * dx + c * dy + cy);
        if sx >= 0.0 && sy >= 0.0 && sx < w && sy < h {
            *p = bilinear(img, sx - 0.5, sy - 0.5);
        }
    }
    out
}

/// Top-left corner for an overlay anchored at a nine-grid position.
pub fn anchor(
    base: (u32, u32),
    top: (u32, u32),
    at: deft_core::value::Position,
    margin: u32,
) -> (i64, i64) {
    let room = |b: u32, t: u32| b as f64 - t as f64 - 2.0 * margin as f64;
    (
        margin as i64 + (room(base.0, top.0) * at.x).round() as i64,
        margin as i64 + (room(base.1, top.1) * at.y).round() as i64,
    )
}

/// Blend `top` over `base` at (x, y) with an extra opacity from 0 to 1.
pub fn overlay(base: &mut RgbaImage, top: &RgbaImage, x: i64, y: i64, opacity: f32) {
    let mut faded = top.clone();
    for p in faded.pixels_mut() {
        p[3] = (p[3] as f32 * opacity).round() as u8;
    }
    image::imageops::overlay(base, &faded, x, y);
}

/// Render an SVG. `width` scales it; otherwise the SVG's own size is used.
pub fn render_svg(bytes: &[u8], width: Option<u32>) -> Result<DynamicImage> {
    let opt = resvg::usvg::Options {
        fontdb: font_database(),
        ..Default::default()
    };
    let tree = resvg::usvg::Tree::from_data(bytes, &opt)
        .map_err(|e| err(ErrorCode::InputCorrupt, format!("svg: {e}")))?;
    let size = tree.size();
    let scale = width.map(|w| w as f32 / size.width()).unwrap_or(1.0);
    let pw = ((size.width() * scale).ceil() as u32).max(1);
    let ph = ((size.height() * scale).ceil() as u32).max(1);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(pw, ph)
        .ok_or_else(|| err(ErrorCode::LimitExceeded, "svg is too large"))?;
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    // tiny-skia stores premultiplied RGBA.
    let mut data = pixmap.take();
    unpremultiply(&mut data);
    let img =
        RgbaImage::from_raw(pw, ph, data).ok_or_else(|| err(ErrorCode::Internal, "svg buffer"))?;
    Ok(DynamicImage::ImageRgba8(img))
}

/// Lossless PNG optimisation.
pub fn optimize_png(bytes: &[u8], strip: bool) -> Result<Vec<u8>> {
    let mut opts = oxipng::Options::from_preset(2);
    opts.strip = if strip {
        oxipng::StripChunks::Safe
    } else {
        oxipng::StripChunks::None
    };
    oxipng::optimize_from_memory(bytes, &opts)
        .map_err(|e| err(ErrorCode::InputCorrupt, format!("png: {e}")))
}

/// Remove metadata without re-encoding. JPEG: drops APPn (except JFIF and,
/// when `keep_icc`, the ICC profile) and comments, keeping the orientation in
/// a minimal EXIF block so the image still displays upright. PNG: drops text,
/// time and EXIF chunks. Returns None for other formats.
pub fn strip_metadata(bytes: &[u8], format: Format, keep_icc: bool) -> Option<Vec<u8>> {
    match format {
        Format::Jpeg => strip_jpeg(bytes, keep_icc),
        Format::Png => strip_png(bytes, keep_icc),
        _ => None,
    }
}

fn strip_jpeg(bytes: &[u8], keep_icc: bool) -> Option<Vec<u8>> {
    let orientation = exif_orientation(bytes);
    let mut out = vec![0xFF, 0xD8];
    if orientation != 1 {
        out.extend_from_slice(&minimal_exif(orientation));
    }
    let mut i = 2;
    while i + 4 <= bytes.len() {
        if bytes[i] != 0xFF {
            return None;
        }
        let marker = bytes[i + 1];
        if marker == 0xDA {
            // Start of scan: copy the rest verbatim.
            out.extend_from_slice(&bytes[i..]);
            return Some(out);
        }
        let len = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        let seg = bytes.get(i..i + 2 + len)?;
        let payload = &seg[4..];
        let keep = match marker {
            0xE0 => payload.starts_with(b"JFIF\0") && orientation == 1,
            0xE2 => keep_icc && payload.starts_with(b"ICC_PROFILE\0"),
            0xE1 | 0xE3..=0xEF | 0xFE => false,
            _ => true,
        };
        if keep {
            out.extend_from_slice(seg);
        }
        i += 2 + len;
    }
    None
}

/// An APP1 segment containing only the EXIF orientation tag.
pub fn minimal_exif(orientation: u8) -> Vec<u8> {
    let mut tiff = Vec::new();
    tiff.extend_from_slice(b"MM\0\x2A\0\0\0\x08"); // big-endian TIFF header, IFD at 8
    tiff.extend_from_slice(&[0, 1]); // one entry
    tiff.extend_from_slice(&[0x01, 0x12, 0, 3, 0, 0, 0, 1, 0, orientation, 0, 0]); // Orientation SHORT 1
    tiff.extend_from_slice(&[0, 0, 0, 0]); // no next IFD
    let mut seg = vec![0xFF, 0xE1];
    seg.extend_from_slice(&((2 + 6 + tiff.len()) as u16).to_be_bytes());
    seg.extend_from_slice(b"Exif\0\0");
    seg.extend_from_slice(&tiff);
    seg
}

fn strip_png(bytes: &[u8], keep_icc: bool) -> Option<Vec<u8>> {
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return None;
    }
    let mut out = bytes[..8].to_vec();
    let mut i = 8;
    while i + 12 <= bytes.len() {
        let len = u32::from_be_bytes(bytes[i..i + 4].try_into().ok()?) as usize;
        let kind = &bytes[i + 4..i + 8];
        let end = i + 12 + len;
        let chunk = bytes.get(i..end)?;
        let drop = matches!(kind, b"tEXt" | b"zTXt" | b"iTXt" | b"tIME" | b"eXIf")
            || (!keep_icc && kind == b"iCCP");
        if !drop {
            out.extend_from_slice(chunk);
        }
        i = end;
        if kind == b"IEND" {
            break;
        }
    }
    Some(out)
}

/// (has EXIF/text metadata, has GPS) from a cheap scan.
pub fn metadata_summary(bytes: &[u8], format: Format) -> (bool, bool) {
    let has = |needle: &[u8]| bytes.windows(needle.len()).any(|w| w == needle);
    match format {
        Format::Jpeg => {
            let exif = has(b"Exif\0\0");
            // GPS IFD pointer tag 0x8825 (type LONG) in either byte order.
            let gps = exif && (has(&[0x88, 0x25, 0x00, 0x04]) || has(&[0x25, 0x88, 0x04, 0x00]));
            (exif, gps)
        }
        Format::Png => (
            has(b"eXIf") || has(b"tEXt") || has(b"iTXt") || has(b"zTXt"),
            false,
        ),
        _ => (false, false),
    }
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;
