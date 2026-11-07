//! QR codes and barcodes.

use deft_core::error::{ErrorCode, Result, err};
use image::{DynamicImage, GrayImage, Rgba, RgbaImage};

/// A code as a grid of modules; `true` is dark.
#[derive(Clone, Debug)]
pub struct Modules {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<bool>,
    /// Blank modules required around the code.
    pub quiet: usize,
}

/// Error correction: `l`, `m`, `q` or `h`.
pub fn qr(text: &str, correction: &str) -> Result<Modules> {
    use qrcode::{Color, EcLevel, QrCode};
    let level = match correction {
        "l" => EcLevel::L,
        "q" => EcLevel::Q,
        "h" => EcLevel::H,
        _ => EcLevel::M,
    };
    let code = QrCode::with_error_correction_level(text.as_bytes(), level).map_err(|e| {
        err(
            ErrorCode::BadValue,
            format!("text does not fit in a QR code: {e}"),
        )
        .hint("--correction l holds the most text")
    })?;
    let width = code.width();
    Ok(Modules {
        width,
        height: width,
        cells: code.to_colors().iter().map(|c| *c == Color::Dark).collect(),
        quiet: 4,
    })
}

/// A linear barcode. `kind` is code128, code39, ean13, ean8 or itf.
pub fn barcode(text: &str, kind: &str) -> Result<Modules> {
    use barcoders::sym::{code39::Code39, code128::Code128, ean8::EAN8, ean13::EAN13, tf::TF};
    let bad = |e: barcoders::error::Error| {
        err(
            ErrorCode::BadValue,
            format!("{text:?} cannot be a {kind} barcode: {e}"),
        )
    };
    let bars = match kind {
        // Character set B: letters, digits and punctuation.
        "code128" => Code128::new(format!("\u{0181}{text}"))
            .map_err(bad)?
            .encode(),
        "code39" => Code39::new(text).map_err(bad)?.encode(),
        "ean13" => EAN13::new(text).map_err(bad)?.encode(),
        "ean8" => EAN8::new(text).map_err(bad)?.encode(),
        "itf" => TF::interleaved(text).map_err(bad)?.encode(),
        other => return Err(err(ErrorCode::BadValue, format!("unknown barcode {other}"))),
    };
    let width = bars.len();
    let height = (width / 3).max(40);
    let row: Vec<bool> = bars.iter().map(|b| *b == 1).collect();
    Ok(Modules {
        width,
        height,
        cells: row.iter().copied().cycle().take(width * height).collect(),
        quiet: 10,
    })
}

/// Draw modules at `scale` pixels each.
pub fn draw(m: &Modules, scale: u32, dark: [u8; 4], light: [u8; 4]) -> DynamicImage {
    let q = m.quiet as u32;
    let (w, h) = (
        (m.width as u32 + 2 * q) * scale,
        (m.height as u32 + 2 * q) * scale,
    );
    let mut out = RgbaImage::from_pixel(w, h, Rgba(light));
    for y in 0..m.height {
        for x in 0..m.width {
            if m.cells[y * m.width + x] {
                for dy in 0..scale {
                    for dx in 0..scale {
                        out.put_pixel(
                            (x as u32 + q) * scale + dx,
                            (y as u32 + q) * scale + dy,
                            Rgba(dark),
                        );
                    }
                }
            }
        }
    }
    DynamicImage::ImageRgba8(out)
}

fn css(c: [u8; 4]) -> String {
    format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])
}

/// Vector version of `draw`, one path for all dark modules.
pub fn svg(m: &Modules, dark: [u8; 4], light: [u8; 4]) -> String {
    let q = m.quiet;
    let (w, h) = (m.width + 2 * q, m.height + 2 * q);
    let mut path = String::new();
    for y in 0..m.height {
        let mut x = 0;
        while x < m.width {
            if m.cells[y * m.width + x] {
                let start = x;
                while x < m.width && m.cells[y * m.width + x] {
                    x += 1;
                }
                path.push_str(&format!(
                    "M{} {}h{}v1h-{}z",
                    start + q,
                    y + q,
                    x - start,
                    x - start
                ));
            } else {
                x += 1;
            }
        }
    }
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\" shape-rendering=\"crispEdges\"><rect width=\"{w}\" height=\"{h}\" fill=\"{}\"/><path d=\"{path}\" fill=\"{}\"/></svg>\n",
        css(light),
        css(dark)
    )
}

/// Decode every QR code in a picture.
pub fn read_qr(img: &DynamicImage) -> Vec<String> {
    let try_read = |gray: GrayImage| -> Vec<String> {
        let mut prepared = rqrr::PreparedImage::prepare(gray);
        prepared
            .detect_grids()
            .iter()
            .filter_map(|g| g.decode().ok().map(|(_, text)| text))
            .collect()
    };
    let found = try_read(img.to_luma8());
    if !found.is_empty() {
        return found;
    }
    // Small codes in large photos and light-on-dark codes need a second look.
    let mut inverted = img.to_luma8();
    image::imageops::invert(&mut inverted);
    let found = try_read(inverted);
    if !found.is_empty() {
        return found;
    }
    let bigger = img.resize(
        img.width() * 2,
        img.height() * 2,
        image::imageops::FilterType::Triangle,
    );
    try_read(bigger.to_luma8())
}
