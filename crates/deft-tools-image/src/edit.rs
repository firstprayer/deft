//! Pixel-level edits: filters, canvas changes, montage, comparison, animation.

use crate::{anchor, overlay};
use deft_core::error::{ErrorCode, Result, err};
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};

/// Gaussian blur of the whole image, or of `region` (x, y, w, h) only.
pub fn blur(img: &DynamicImage, radius: f32, region: Option<(u32, u32, u32, u32)>) -> DynamicImage {
    let Some((x, y, w, h)) = region else {
        return img.fast_blur(radius);
    };
    let part = img.crop_imm(x, y, w, h).fast_blur(radius);
    let mut out = img.to_rgba8();
    image::imageops::replace(&mut out, &part.to_rgba8(), x as i64, y as i64);
    DynamicImage::ImageRgba8(out)
}

pub fn sharpen(img: &DynamicImage, radius: f32) -> DynamicImage {
    img.unsharpen(radius, 2)
}

/// Brightness, contrast and saturation in percent (0 changes nothing, -100 to
/// 100), then gamma (1 changes nothing).
pub fn adjust(
    img: &DynamicImage,
    brightness: f32,
    contrast: f32,
    saturation: f32,
    gamma: f32,
) -> DynamicImage {
    let mut out = img.to_rgba8();
    let contrast = (100.0 + contrast) / 100.0;
    let saturation = (100.0 + saturation) / 100.0;
    let gain = 1.0 + brightness / 100.0;
    let table: Vec<f32> = (0..=255)
        .map(|v| {
            let mut x = v as f32 * gain;
            x = (x - 127.5) * contrast + 127.5;
            x = x.clamp(0.0, 255.0);
            255.0 * (x / 255.0).powf(1.0 / gamma)
        })
        .collect();
    for p in out.pixels_mut() {
        let [r, g, b, a] = p.0;
        let c = [table[r as usize], table[g as usize], table[b as usize]];
        let luma = 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
        let v = c.map(|x| (luma + (x - luma) * saturation).clamp(0.0, 255.0).round() as u8);
        p.0 = [v[0], v[1], v[2], a];
    }
    DynamicImage::ImageRgba8(out)
}

fn distance(a: [u8; 4], b: [u8; 4]) -> u8 {
    (0..3)
        .map(|i| a[i].abs_diff(b[i]))
        .max()
        .unwrap_or(0)
        .max(a[3].abs_diff(b[3]))
}

/// The colour most of the image's outer edge has. One corner alone could be a
/// speck, or a mark the border does not share.
fn border_colour(rgba: &RgbaImage) -> [u8; 4] {
    let (w, h) = rgba.dimensions();
    let mut seen = std::collections::HashMap::new();
    let edge = (0..w)
        .flat_map(|x| [(x, 0), (x, h - 1)])
        .chain((0..h).flat_map(|y| [(0, y), (w - 1, y)]));
    for (x, y) in edge {
        *seen.entry(rgba.get_pixel(x, y).0).or_insert(0u32) += 1;
    }
    seen.into_iter()
        .max_by_key(|(colour, n)| (*n, *colour))
        .map_or([0; 4], |(colour, _)| colour)
}

/// Clear every patch of touching marks of at most `speck` pixels. A patch is
/// measured whole, so a thin rule stays however thin it is.
fn drop_specks(marks: &mut [bool], w: usize, speck: usize) {
    let mut seen = vec![false; marks.len()];
    for start in 0..marks.len() {
        if !marks[start] || seen[start] {
            continue;
        }
        seen[start] = true;
        let (mut patch, mut next) = (vec![start], 0);
        while next < patch.len() {
            let (x, y) = (patch[next] % w, patch[next] / w);
            next += 1;
            for ny in y.saturating_sub(1)..=y + 1 {
                for nx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    let i = ny * w + nx;
                    if i < marks.len() && marks[i] && !seen[i] {
                        seen[i] = true;
                        patch.push(i);
                    }
                }
            }
        }
        if patch.len() <= speck {
            patch.into_iter().for_each(|i| marks[i] = false);
        }
    }
}

/// Bounds of what differs from the border colour, within a tolerance from 0 to
/// 100, leaving out patches of at most `speck` pixels. Returns x, y, w, h.
pub fn trim_box(img: &DynamicImage, fuzz: f32, speck: usize) -> Option<(u32, u32, u32, u32)> {
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    if w == 0 || h == 0 {
        return None;
    }
    let edge = border_colour(&rgba);
    let limit = (fuzz / 100.0 * 255.0) as u8;
    let mut marks: Vec<bool> = rgba.pixels().map(|p| distance(p.0, edge) > limit).collect();
    if speck > 0 {
        drop_specks(&mut marks, w as usize, speck);
    }
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for (i, _) in marks.iter().enumerate().filter(|(_, m)| **m) {
        let (x, y) = (i as u32 % w, i as u32 / w);
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    (x0 <= x1 && y0 <= y1).then(|| (x0, y0, x1 + 1 - x0, y1 + 1 - y0))
}

/// Place an image on a larger canvas.
pub fn pad(
    img: &DynamicImage,
    w: u32,
    h: u32,
    background: [u8; 4],
    position: deft_core::value::Position,
) -> DynamicImage {
    let mut out = RgbaImage::from_pixel(w, h, Rgba(background));
    let top = img.to_rgba8();
    let (x, y) = anchor((w, h), top.dimensions(), position, 0);
    overlay(&mut out, &top, x, y, 1.0);
    DynamicImage::ImageRgba8(out)
}

/// Make a colour transparent, within a tolerance from 0 to 100. Returns the
/// image and how many pixels changed.
pub fn make_transparent(img: &DynamicImage, color: [u8; 4], fuzz: f32) -> (DynamicImage, usize) {
    let mut out = img.to_rgba8();
    let limit = (fuzz / 100.0 * 255.0) as u8;
    let mut n = 0;
    for p in out.pixels_mut() {
        let [r, g, b, _] = p.0;
        if distance([r, g, b, 255], [color[0], color[1], color[2], 255]) <= limit {
            p.0[3] = 0;
            n += 1;
        }
    }
    (DynamicImage::ImageRgba8(out), n)
}

/// How a grid of pictures is laid out.
#[derive(Clone, Copy, Debug)]
pub struct Grid {
    pub cols: u32,
    /// Size of one tile; `None` uses the largest input.
    pub tile: Option<(u32, u32)>,
    pub gap: u32,
    pub background: [u8; 4],
}

/// Tile images left to right, top to bottom, each fitted into its cell.
pub fn montage(images: &[DynamicImage], grid: Grid) -> Result<DynamicImage> {
    if images.is_empty() {
        return Err(err(ErrorCode::BadOption, "no images"));
    }
    let (tw, th) = grid.tile.unwrap_or_else(|| {
        (
            images.iter().map(|i| i.width()).max().unwrap_or(1),
            images.iter().map(|i| i.height()).max().unwrap_or(1),
        )
    });
    let cols = grid.cols.max(1);
    let rows = (images.len() as u32).div_ceil(cols);
    let (w, h) = (
        cols * tw + (cols + 1) * grid.gap,
        rows * th + (rows + 1) * grid.gap,
    );
    let mut out = RgbaImage::from_pixel(w, h, Rgba(grid.background));
    for (n, img) in images.iter().enumerate() {
        let (c, r) = (n as u32 % cols, n as u32 / cols);
        let spec = deft_core::value::SizeSpec {
            width: Some(tw),
            height: Some(th),
            percent: None,
            source: String::new(),
        };
        let fitted = crate::resize(img, &spec, crate::Fit::Contain, true)?;
        let x = grid.gap + c * (tw + grid.gap) + (tw - fitted.width().min(tw)) / 2;
        let y = grid.gap + r * (th + grid.gap) + (th - fitted.height().min(th)) / 2;
        overlay(&mut out, &fitted.to_rgba8(), x as i64, y as i64, 1.0);
    }
    Ok(DynamicImage::ImageRgba8(out))
}

/// How two images differ.
#[derive(Clone, Debug)]
pub struct Difference {
    /// Share of pixels that differ visibly, 0 to 1.
    pub changed: f64,
    /// Mean difference per channel, 0 to 1.
    pub mean: f64,
    /// The first image dimmed, with changed pixels in red.
    pub picture: DynamicImage,
}

/// Compare two images of the same size.
pub fn compare(a: &DynamicImage, b: &DynamicImage, fuzz: f32) -> Result<Difference> {
    if a.dimensions() != b.dimensions() {
        return Err(err(
            ErrorCode::BadValue,
            format!(
                "sizes differ: {}x{} and {}x{}",
                a.width(),
                a.height(),
                b.width(),
                b.height()
            ),
        ));
    }
    let (x, y) = (a.to_rgba8(), b.to_rgba8());
    let limit = (fuzz / 100.0 * 255.0) as u8;
    let mut picture = RgbaImage::new(x.width(), x.height());
    let (mut changed, mut sum) = (0u64, 0u64);
    for ((p, q), o) in x.pixels().zip(y.pixels()).zip(picture.pixels_mut()) {
        for i in 0..3 {
            sum += p.0[i].abs_diff(q.0[i]) as u64;
        }
        if distance(p.0, q.0) > limit {
            changed += 1;
            o.0 = [255, 0, 0, 255];
        } else {
            let dim = |v: u8| 255 - (255 - v) / 4;
            o.0 = [dim(p.0[0]), dim(p.0[1]), dim(p.0[2]), 255];
        }
    }
    let pixels = (x.width() as u64 * x.height() as u64).max(1);
    Ok(Difference {
        changed: changed as f64 / pixels as f64,
        mean: sum as f64 / (pixels * 3 * 255) as f64,
        picture: DynamicImage::ImageRgba8(picture),
    })
}

/// Encode frames as an animated GIF. `delay_ms` applies to every frame;
/// `repeat` of 0 loops forever.
pub fn encode_gif(frames: Vec<RgbaImage>, delay_ms: u32, repeat: u16) -> Result<Vec<u8>> {
    use image::codecs::gif::{GifEncoder, Repeat};
    let bad = |e: image::ImageError| err(ErrorCode::Internal, format!("gif: {e}"));
    let mut out = Vec::new();
    {
        let mut enc = GifEncoder::new_with_speed(&mut out, 10);
        enc.set_repeat(if repeat == 0 {
            Repeat::Infinite
        } else {
            Repeat::Finite(repeat)
        })
        .map_err(bad)?;
        for f in frames {
            let delay = image::Delay::from_numer_denom_ms(delay_ms, 1);
            enc.encode_frame(image::Frame::from_parts(f, 0, 0, delay))
                .map_err(bad)?;
        }
    }
    Ok(out)
}

/// The frames of an animated GIF, composed to full size.
pub fn gif_frames(bytes: &[u8]) -> Result<Vec<RgbaImage>> {
    use image::AnimationDecoder;
    let decoder = image::codecs::gif::GifDecoder::new(std::io::Cursor::new(bytes))
        .map_err(|e| err(ErrorCode::InputCorrupt, e.to_string()))?;
    decoder
        .into_frames()
        .map(|f| {
            f.map(|f| f.into_buffer())
                .map_err(|e| err(ErrorCode::InputCorrupt, e.to_string()))
        })
        .collect()
}

/// A multi-resolution ICO file.
pub fn encode_ico(img: &DynamicImage, sizes: &[u32]) -> Result<Vec<u8>> {
    use image::codecs::ico::{IcoEncoder, IcoFrame};
    let bad = |e: image::ImageError| err(ErrorCode::Internal, format!("ico: {e}"));
    let mut frames = Vec::new();
    for &s in sizes {
        let small = crate::resize_exact(img, s, s)?.to_rgba8();
        frames.push(
            IcoFrame::as_png(small.as_raw(), s, s, image::ExtendedColorType::Rgba8).map_err(bad)?,
        );
    }
    let mut out = Vec::new();
    IcoEncoder::new(&mut out)
        .encode_images(&frames)
        .map_err(bad)?;
    Ok(out)
}
