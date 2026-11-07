use super::*;

fn sample_jpeg(orientation: u8) -> Vec<u8> {
    let img = DynamicImage::ImageRgb8(RgbImage::from_fn(40, 20, |x, _| {
        image::Rgb([(x * 6) as u8, 10, 200])
    }));
    let mut plain = encode(&img, Format::Jpeg, Encode::default()).unwrap();
    if orientation != 1 {
        plain.splice(2..2, minimal_exif(orientation));
    }
    plain
}

#[test]
fn orientation_is_applied_and_kept_when_stripping() {
    let jpg = sample_jpeg(6);
    assert_eq!(exif_orientation(&jpg), 6);
    let img = decode(&jpg, Format::Jpeg, u64::MAX).unwrap();
    assert_eq!(img.dimensions(), (20, 40)); // rotated upright
    let stripped = strip_metadata(&jpg, Format::Jpeg, true).unwrap().bytes;
    assert_eq!(exif_orientation(&stripped), 6);
    assert_eq!(jpeg_info(&stripped), Some((40, 20, 3)));
    assert!(decode(&stripped, Format::Jpeg, u64::MAX).is_ok());
}

#[test]
fn resize_modes() {
    let img = DynamicImage::ImageRgb8(RgbImage::new(400, 200));
    let spec = deft_core::value::SizeSpec::parse("100x100").unwrap();
    assert_eq!(
        resize(&img, &spec, Fit::Contain, false)
            .unwrap()
            .dimensions(),
        (100, 50)
    );
    assert_eq!(
        resize(&img, &spec, Fit::Cover, false).unwrap().dimensions(),
        (100, 100)
    );
    assert_eq!(
        resize(&img, &spec, Fit::Fill, false).unwrap().dimensions(),
        (100, 100)
    );
    let big = deft_core::value::SizeSpec::parse("800x").unwrap();
    assert_eq!(
        resize(&img, &big, Fit::Contain, false)
            .unwrap()
            .dimensions(),
        (400, 200)
    );
    assert_eq!(
        resize(&img, &big, Fit::Contain, true).unwrap().dimensions(),
        (800, 400)
    );
}

#[test]
fn encodes_every_writable_format() {
    let img = DynamicImage::ImageRgba8(RgbaImage::from_fn(16, 16, |x, y| {
        image::Rgba([x as u8 * 16, y as u8 * 16, 0, 200])
    }));
    for f in WRITABLE {
        let bytes = encode(&img, *f, Encode::default()).unwrap();
        assert_eq!(deft_core::format::sniff(&bytes, None), Some(*f), "{f}");
    }
}

#[test]
fn text_renders_in_any_script() {
    let style = TextStyle::new(40.0, [200, 0, 0, 255]);
    for text in ["Sample", "机密 文件", "Line one\nline two"] {
        let img = render_text(text, &style).unwrap();
        assert!(
            img.width() > 30 && img.height() > 20,
            "{text}: {:?}",
            img.dimensions()
        );
        assert!(
            img.pixels().any(|p| p[3] > 200 && p[0] > 150),
            "{text} has ink"
        );
    }
    let tilted = render_text(
        "Sample",
        &TextStyle {
            angle: 45.0,
            ..style
        },
    )
    .unwrap();
    assert!(
        tilted.height() > 80,
        "rotated text is taller: {:?}",
        tilted.dimensions()
    );
}

#[test]
fn wrapping() {
    assert_eq!(wrap_text("one two three", 5.0), ["one two", "three"]);
    assert_eq!(wrap_text("这是一段比较长的中文字幕", 6.0).len(), 3);
    assert_eq!(wrap_text("a\nb", 50.0), ["a", "b"]);
}

#[test]
fn overlays_blend_and_anchor() {
    use deft_core::value::Position;
    assert_eq!(
        anchor((100, 100), (10, 10), Position { x: 1.0, y: 1.0 }, 5),
        (85, 85)
    );
    assert_eq!(
        anchor((100, 100), (10, 10), Position { x: 0.5, y: 0.5 }, 0),
        (45, 45)
    );
    let mut base = RgbaImage::from_pixel(10, 10, image::Rgba([0, 0, 0, 255]));
    overlay(
        &mut base,
        &RgbaImage::from_pixel(2, 2, image::Rgba([255, 255, 255, 255])),
        0,
        0,
        0.5,
    );
    assert!((120..=135).contains(&base.get_pixel(0, 0)[0]));
    assert_eq!(base.get_pixel(5, 5)[0], 0);
    let r = rotate_any(
        &RgbaImage::from_pixel(20, 10, image::Rgba([1, 2, 3, 255])),
        90.0,
    );
    assert_eq!(r.dimensions(), (10, 20));
}

#[test]
fn svg_renders() {
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" width="20" height="10"><rect width="20" height="10" fill="red"/></svg>"#;
    let img = render_svg(svg, Some(40)).unwrap();
    assert_eq!(img.dimensions(), (40, 20));
}

#[test]
fn strip_keeps_the_adobe_marker_that_says_how_colours_are_stored() {
    let mut jpg = sample_jpeg(1);
    let adobe = b"\xFF\xEE\x00\x0EAdobe\x00\x64\x00\x00\x00\x00\x02";
    jpg.splice(2..2, adobe.iter().copied());
    let stripped = strip_metadata(&jpg, Format::Jpeg, false).unwrap();
    assert!(stripped.bytes.windows(adobe.len()).any(|w| w == adobe));
}

#[test]
fn strip_finds_the_end_of_a_jpeg_past_markers_inside_the_scan() {
    let jpg = sample_jpeg(1);
    let extra = sample_jpeg(1);
    let both = [jpg.clone(), extra.clone()].concat();
    let stripped = strip_metadata(&both, Format::Jpeg, true).unwrap();
    assert_eq!(stripped.trailing, extra.len());
    assert_eq!(stripped.bytes.len(), jpg.len());
}

#[test]
fn a_cut_jpeg_keeps_what_it_has() {
    let jpg = sample_jpeg(1);
    let cut = &jpg[..jpg.len() - 10];
    let stripped = strip_metadata(cut, Format::Jpeg, true).unwrap();
    assert_eq!(stripped.bytes, cut);
}

#[test]
fn a_profile_only_travels_with_pixels_of_its_colour_model() {
    let mut icc = vec![0u8; 128];
    icc[16..20].copy_from_slice(b"CMYK");
    let rgb = DynamicImage::ImageRgb8(RgbImage::new(4, 4));
    assert!(!profile_fits(&icc, &rgb));
    icc[16..20].copy_from_slice(b"RGB ");
    assert!(profile_fits(&icc, &rgb));
    assert!(!profile_fits(
        &icc,
        &DynamicImage::ImageLuma8(image::GrayImage::new(4, 4))
    ));
}

#[test]
fn an_unusable_profile_leaves_the_pixels_alone() {
    let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(4, 4, image::Rgb([9, 8, 7])));
    let mut icc = vec![0u8; 128];
    icc[16..20].copy_from_slice(b"RGB ");
    let opt = Encode {
        icc: Some(&icc),
        ..Default::default()
    };
    assert!(encode(&img, Format::Bmp, opt).is_ok());
}
