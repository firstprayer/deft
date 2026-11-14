use super::*;

const BLACK: [u8; 4] = [0, 0, 0, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];

/// A QR code holding raw bytes, as a Japanese phone app writes Shift-JIS.
fn modules_of(bytes: &[u8]) -> Modules {
    let code = qrcode::QrCode::new(bytes).unwrap();
    let width = code.width();
    Modules {
        width,
        height: width,
        cells: code
            .to_colors()
            .iter()
            .map(|c| *c == qrcode::Color::Dark)
            .collect(),
        quiet: 4,
    }
}

#[test]
fn every_code_in_a_picture_is_read() {
    let a = draw(&qr("https://example.com/a", "m").unwrap(), 6, BLACK, WHITE).to_rgba8();
    let b = draw(&qr("second code", "m").unwrap(), 6, BLACK, WHITE).to_rgba8();
    let (w, h) = (a.width() + b.width() + 60, a.height().max(b.height()));
    let mut both = RgbaImage::from_pixel(w, h, Rgba(WHITE));
    image::imageops::overlay(&mut both, &a, 0, 0);
    image::imageops::overlay(&mut both, &b, (a.width() + 60) as i64, 0);
    let mut found = read_qr(&DynamicImage::ImageRgba8(both));
    found.sort();
    assert_eq!(found, ["https://example.com/a", "second code"]);
}

#[test]
fn a_transparent_background_counts_as_white() {
    let img = draw(&qr("on glass", "m").unwrap(), 5, BLACK, [0, 0, 0, 0]);
    assert_eq!(read_qr(&img), ["on glass"]);
}

#[test]
fn shift_jis_text_is_decoded() {
    // "ホテル" in half-width katakana.
    let img = draw(&modules_of(b"\xCE\xC3\xD9 hotel"), 5, BLACK, WHITE);
    assert_eq!(read_qr(&img), ["\u{FF8E}\u{FF83}\u{FF99} hotel"]);
}

#[test]
fn latin_1_and_utf_8_are_not_taken_for_shift_jis() {
    assert!(looks_shift_jis(b"\xCE\xC3\xD9;URL"));
    assert!(!looks_shift_jis("café".as_bytes()));
    assert!(!looks_shift_jis(b"caf\xE9 cr\xE8me"));
    assert!(!looks_shift_jis(b"plain"));
}

#[test]
fn a_picture_with_no_code_gives_none() {
    let img = RgbaImage::from_pixel(300, 200, Rgba([200, 30, 30, 255]));
    assert!(read_qr(&DynamicImage::ImageRgba8(img)).is_empty());
}
