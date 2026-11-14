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

/// Decode every QR code in a picture, each text once, in the order found.
///
/// ZXing's reader (rxing) looks at the picture several ways, and every way is
/// tried, because a photo with two codes often gives up one per look: both
/// binarizers, light-on-dark, a smaller or larger copy for codes that are tiny
/// in a big photo or drawn a few pixels per module, and each corner of the
/// photo alone. Transparent pixels count as white, as a viewer shows them.
pub fn read_qr(img: &DynamicImage) -> Vec<String> {
    let gray = on_white(img);
    let (w, h) = gray.dimensions();
    let long = w.max(h);
    let mut looks = vec![gray.clone()];
    if long > 1600 {
        let scale = 1200.0 / long as f32;
        looks.push(image::imageops::resize(
            &gray,
            ((w as f32 * scale) as u32).max(1),
            ((h as f32 * scale) as u32).max(1),
            image::imageops::FilterType::Triangle,
        ));
    }
    if long < 1000 {
        looks.push(image::imageops::resize(
            &gray,
            w * 2,
            h * 2,
            image::imageops::FilterType::Triangle,
        ));
    }
    // Each part of a photo alone, where a second code was hidden by the first.
    let base = if long > 4000 { &looks[1] } else { &gray }.clone();
    let (bw, bh) = base.dimensions();
    if bw.min(bh) >= 300 {
        let (tw, th) = (bw * 3 / 5, bh * 3 / 5);
        for (x, y) in [(0, 0), (bw - tw, 0), (0, bh - th), (bw - tw, bh - th)] {
            looks.push(image::imageops::crop_imm(&base, x, y, tw, th).to_image());
        }
    }
    let mut inverted = gray;
    image::imageops::invert(&mut inverted);
    looks.push(inverted);

    let mut found: Vec<String> = Vec::new();
    for look in looks {
        for text in read_look(look) {
            if !found.contains(&text) {
                found.push(text);
            }
        }
    }
    found
}

/// Gray with transparency flattened onto white.
fn on_white(img: &DynamicImage) -> GrayImage {
    if !img.color().has_alpha() {
        return img.to_luma8();
    }
    let rgba = img.to_rgba8();
    GrayImage::from_fn(rgba.width(), rgba.height(), |x, y| {
        let [r, g, b, a] = rgba.get_pixel(x, y).0;
        let luma = (r as f32 * 0.299 + g as f32 * 0.587 + b as f32 * 0.114) / 255.0;
        let alpha = a as f32 / 255.0;
        image::Luma([((luma * alpha + (1.0 - alpha)) * 255.0).round() as u8])
    })
}

/// Every code rxing finds in one picture, with both of its binarizers.
fn read_look(gray: GrayImage) -> Vec<String> {
    use rxing::common::{GlobalHistogramBinarizer, HybridBinarizer};
    use rxing::multi::MultipleBarcodeReader;
    use rxing::multi::qrcode::QRCodeMultiReader;
    use rxing::qrcode::QRCodeReader;
    use rxing::{BarcodeFormat, BinaryBitmap, DecodeHints, Luma8LuminanceSource, Reader};

    let (w, h) = gray.dimensions();
    if w == 0 || h == 0 {
        return vec![];
    }
    let hints = DecodeHints {
        TryHarder: Some(true),
        PossibleFormats: Some([BarcodeFormat::QR_CODE].into_iter().collect()),
        ..Default::default()
    };
    let luma = gray.into_raw();
    let mut texts = Vec::new();
    let mut take = |results: Vec<rxing::RXingResult>| {
        texts.extend(results.iter().map(text_of));
    };
    let source = || Luma8LuminanceSource::new(luma.clone(), w, h);
    if let Ok(src) = source() {
        let mut bitmap = BinaryBitmap::new(HybridBinarizer::new(src));
        take(
            QRCodeMultiReader::new()
                .decode_multiple_with_hints(&mut bitmap, &hints)
                .unwrap_or_default(),
        );
        if let Ok(one) = QRCodeReader::new().decode_with_hints(&mut bitmap, &hints) {
            take(vec![one]);
        }
    }
    if let Ok(src) = source() {
        let mut bitmap = BinaryBitmap::new(GlobalHistogramBinarizer::new(src));
        take(
            QRCodeMultiReader::new()
                .decode_multiple_with_hints(&mut bitmap, &hints)
                .unwrap_or_default(),
        );
        if let Ok(one) = QRCodeReader::new().decode_with_hints(&mut bitmap, &hints) {
            take(vec![one]);
        }
    }
    texts
}

/// The text of a code. rxing 0.9.3 never recognises half-width katakana when
/// it guesses how a byte segment is encoded, so a Japanese code such as
/// "筑波ﾎﾃﾙ" comes out as Latin-1 ("筑波ÎÃÙ"). Such a segment is decoded again
/// with ZXing's own rule for Shift-JIS.
fn text_of(result: &rxing::RXingResult) -> String {
    use rxing::{RXingResultMetadataType, RXingResultMetadataValue};
    let mut text = result.getText().to_string();
    let segments = result
        .getRXingResultMetadata()
        .get(&RXingResultMetadataType::BYTE_SEGMENTS);
    let Some(RXingResultMetadataValue::ByteSegments(segments)) = segments else {
        return text;
    };
    for bytes in segments {
        if !looks_shift_jis(bytes) {
            continue;
        }
        let latin: String = bytes.iter().map(|b| *b as char).collect();
        let (sjis, _, bad) = encoding_rs::SHIFT_JIS.decode(bytes);
        if !bad && text.contains(&latin) {
            text = text.replacen(&latin, &sjis, 1);
        }
    }
    text
}

/// ZXing's test for bytes that are Shift-JIS rather than Latin-1: valid as
/// Shift-JIS, and three katakana or double-byte characters in a row, or only
/// two katakana, or many bytes Latin-1 would show as rare symbols.
fn looks_shift_jis(bytes: &[u8]) -> bool {
    if bytes.is_ascii() || std::str::from_utf8(bytes).is_ok() {
        return false;
    }
    let (mut left, mut kana, mut kana_run, mut kana_max) = (0, 0, 0, 0);
    let (mut double_run, mut double_max, mut latin_rare) = (0, 0, 0);
    for &b in bytes {
        if (0xA0..0xC0).contains(&b) || b == 0xD7 || b == 0xF7 {
            latin_rare += 1;
        }
        if left > 0 {
            if matches!(b, 0x40 | 0x7F | 0xFC) {
                return false;
            }
            left -= 1;
        } else if matches!(b, 0x80 | 0xA0 | 0xEF) {
            return false;
        } else if b > 0xA0 && b < 0xE0 {
            kana += 1;
            double_run = 0;
            kana_run += 1;
            kana_max = kana_max.max(kana_run);
        } else if b > 0x7F {
            left += 1;
            kana_run = 0;
            double_run += 1;
            double_max = double_max.max(double_run);
        } else {
            kana_run = 0;
            double_run = 0;
        }
    }
    left == 0
        && (kana_max >= 3
            || double_max >= 3
            || (kana_max == 2 && kana == 2)
            || latin_rare * 10 >= bytes.len())
}

#[cfg(test)]
#[path = "codes_tests.rs"]
mod tests;
