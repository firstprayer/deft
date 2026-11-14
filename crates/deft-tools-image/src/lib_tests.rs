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

#[test]
fn webp_orientation_behind_an_exif_prefix_is_read() {
    // iPhone WebP files put the JPEG segment's `Exif\0\0` before the TIFF data.
    let mut exif = b"Exif\0\0".to_vec();
    exif.extend_from_slice(&crate::orientation_tiff(6));
    assert_eq!(
        crate::tiff_orientation(exif.strip_prefix(b"Exif\0\0").unwrap()),
        Some(6)
    );
    let little = b"II\x2A\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x08\0\0\0\0\0\0\0";
    assert_eq!(crate::tiff_orientation(little), Some(8));
    // Every command reads it, not only strip: a 4x2 picture shows 2x4.
    let img = DynamicImage::ImageRgb8(image::RgbImage::new(4, 2));
    let plain = encode(&img, Format::Webp, Encode::default()).unwrap();
    let webp = crate::webp::with_exif(&plain, &exif).unwrap();
    assert_eq!(exif_orientation(&webp), 6);
    let upright = decode(&webp, Format::Webp, u64::MAX).unwrap();
    assert_eq!((upright.width(), upright.height()), (2, 4));
}

#[test]
fn a_gif_keeps_its_frames_and_loses_its_comments() {
    // Two 1x1 frames with a comment and an XMP application block between them.
    let mut gif = b"GIF89a\x01\0\x01\0\x80\0\0\0\0\0\xff\xff\xff".to_vec();
    gif.extend_from_slice(b"\x21\xff\x0bNETSCAPE2.0\x03\x01\0\0\0");
    let frame = b"\x21\xf9\x04\0\x0a\0\0\0\x2c\0\0\0\0\x01\0\x01\0\0\x02\x02\x44\x01\0";
    gif.extend_from_slice(frame);
    gif.extend_from_slice(b"\x21\xfe\x09Ada Smith\0");
    gif.extend_from_slice(b"\x21\xff\x0bXMP DataXMP\x05<x/>\0\0");
    gif.extend_from_slice(frame);
    gif.extend_from_slice(b"\x3bTRAILER");
    let s = crate::strip_metadata(&gif, deft_core::format::Format::Gif, true).unwrap();
    let out = &s.bytes;
    assert!(!out.windows(9).any(|w| w == b"Ada Smith"));
    assert!(!out.windows(3).any(|w| w == b"XMP"));
    assert!(out.windows(11).any(|w| w == b"NETSCAPE2.0"));
    assert_eq!(
        out.windows(2).filter(|w| *w == [0x21, 0xf9]).count(),
        2,
        "both frames stay"
    );
    assert_eq!(s.trailing, 7);
}

/// A small CMYK printer profile: ink only darkens, without hue.
fn cmyk_profile() -> Vec<u8> {
    let mut lut = b"mft1\0\0\0\0".to_vec();
    lut.extend_from_slice(&[4, 3, 2, 0]);
    for i in 0..9 {
        let one = if i % 4 == 0 { 0x0001_0000u32 } else { 0 };
        lut.extend_from_slice(&one.to_be_bytes());
    }
    let ramp: Vec<u8> = (0..=255).collect();
    for _ in 0..4 {
        lut.extend_from_slice(&ramp);
    }
    // Grid corners in C, M, Y, K order with K varying fastest.
    for corner in 0..16u8 {
        let inked = corner.count_ones();
        let l = if corner & 1 == 1 {
            0
        } else {
            255 - 60 * inked.min(4)
        };
        lut.extend_from_slice(&[l as u8, 128, 128]);
    }
    for _ in 0..3 {
        lut.extend_from_slice(&ramp);
    }
    let mut icc = vec![0u8; 128];
    icc[8..12].copy_from_slice(&[2, 0x10, 0, 0]);
    icc[12..16].copy_from_slice(b"prtr");
    icc[16..20].copy_from_slice(b"CMYK");
    icc[20..24].copy_from_slice(b"Lab ");
    icc[36..40].copy_from_slice(b"acsp");
    // D50 illuminant.
    icc[68..80].copy_from_slice(&[0, 0, 0xF6, 0xD6, 0, 1, 0, 0, 0, 0, 0xD3, 0x2D]);
    icc.extend_from_slice(&1u32.to_be_bytes());
    icc.extend_from_slice(b"A2B0");
    icc.extend_from_slice(&(128 + 4 + 12u32).to_be_bytes());
    icc.extend_from_slice(&(lut.len() as u32).to_be_bytes());
    icc.extend_from_slice(&lut);
    let size = icc.len() as u32;
    icc[..4].copy_from_slice(&size.to_be_bytes());
    icc
}

#[test]
fn cmyk_goes_through_its_profile() {
    let icc = cmyk_profile();
    let rgb = crate::cmyk_to_srgb(&[0, 0, 0, 0, 0, 0, 0, 255], &icc).unwrap();
    assert!(
        rgb[..3].iter().all(|v| *v > 240),
        "no ink is white: {rgb:?}"
    );
    assert!(
        rgb[3..].iter().all(|v| *v < 15),
        "full black is black: {rgb:?}"
    );
    let adobe = [0xFF, 0xD8, 0xFF, 0xEE, 0, 14, b'A', b'd', b'o', b'b', b'e'];
    assert!(crate::has_adobe_segment(&adobe));
    assert!(!crate::has_adobe_segment(&adobe[..6]));
}

#[test]
fn tags_are_written_into_webp_without_touching_the_picture() {
    let img = DynamicImage::ImageRgb8(RgbImage::from_pixel(6, 3, image::Rgb([10, 200, 30])));
    let plain = encode(&img, Format::Webp, Encode::default()).unwrap();
    let fields = crate::exif::Fields {
        author: Some("Ada".into()),
        ..Default::default()
    };
    let signed = crate::exif::write(&plain, Format::Webp, &fields).unwrap();
    assert_eq!(crate::exif::read(&signed)["Artist"], "Ada");
    let again = decode(&signed, Format::Webp, u64::MAX).unwrap();
    assert_eq!(again.to_rgb8(), img.to_rgb8());
    // A second write replaces the tags instead of adding a second chunk.
    let fields = crate::exif::Fields {
        title: Some("Sea".into()),
        ..Default::default()
    };
    let twice = crate::exif::write(&signed, Format::Webp, &fields).unwrap();
    let tags = crate::exif::read(&twice);
    assert_eq!(
        (tags["Artist"].as_str(), tags["ImageDescription"].as_str()),
        (Some("Ada"), Some("Sea"))
    );
    assert_eq!(twice.windows(4).filter(|w| *w == b"EXIF").count(), 1);
}

#[test]
fn an_svg_comes_out_at_the_width_asked_for() {
    // A fractional size whose rounded height would scale it a pixel wider.
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 613.7 458.5" width="613.7" height="458.5"><rect width="613.7" height="458.5" fill="red"/></svg>"#;
    let spec = deft_core::value::SizeSpec::parse("512x").unwrap();
    let img = render_svg_sized(svg, &spec, Fit::Contain, u64::MAX).unwrap();
    assert_eq!(img.width(), 512);
    assert!((382..=383).contains(&img.height()), "{}", img.height());
}

#[test]
fn invisible_margins_are_trimmed_whatever_colour_they_keep() {
    // A 4x2 red mark in a 10x6 canvas whose transparent pixels hold noise.
    let img = RgbaImage::from_fn(10, 6, |x, y| {
        if (3..7).contains(&x) && (2..4).contains(&y) {
            image::Rgba([200, 0, 0, 255])
        } else {
            image::Rgba([(x * 25) as u8, (y * 40) as u8, 7, 0])
        }
    });
    let b = crate::edit::trim_box(&DynamicImage::ImageRgba8(img), 5.0, 0);
    assert_eq!(b, Some((3, 2, 4, 2)));
}

#[test]
fn only_the_background_joined_to_the_edge_goes() {
    // White background, a black ring, white inside the ring.
    let img = DynamicImage::ImageRgb8(RgbImage::from_fn(9, 9, |x, y| {
        let ring = (2..7).contains(&x)
            && (2..7).contains(&y)
            && !((3..6).contains(&x) && (3..6).contains(&y));
        image::Rgb(if ring { [0, 0, 0] } else { [255, 255, 255] })
    }));
    let (out, n) = crate::edit::make_transparent(&img, [255, 255, 255, 255], 10.0, false);
    let out = out.to_rgba8();
    assert_eq!(out.get_pixel(0, 0)[3], 0);
    assert_eq!(out.get_pixel(4, 4)[3], 255, "the white inside stays");
    assert_eq!(n, 81 - 25);
    let (_, all) = crate::edit::make_transparent(&img, [255, 255, 255, 255], 10.0, true);
    assert_eq!(all, 81 - 16);
    // A scan in a dark border: the paper does not touch the edge, and goes.
    let scan = DynamicImage::ImageRgb8(RgbImage::from_fn(9, 9, |x, y| {
        let border = x == 0 || y == 0 || x == 8 || y == 8;
        image::Rgb(if border || (x, y) == (4, 4) {
            [0, 0, 0]
        } else {
            [255, 255, 255]
        })
    }));
    let (_, paper) = crate::edit::make_transparent(&scan, [255, 255, 255, 255], 10.0, false);
    assert_eq!(paper, 49 - 1);
}

#[test]
fn gps_behind_a_pointer_of_the_ifd_type_is_found() {
    // A little-endian TIFF: IFD0 with a GPS pointer of type 13 (IFD), and a
    // GPS IFD with latitude 49 N, as a Parrot Sequoia camera writes it.
    let mut t = b"II\x2A\0\x08\0\0\0".to_vec();
    t.extend_from_slice(&[1, 0]);
    t.extend_from_slice(&[0x25, 0x88, 13, 0, 1, 0, 0, 0, 26, 0, 0, 0]);
    t.extend_from_slice(&[0; 4]);
    assert_eq!(t.len(), 26);
    t.extend_from_slice(&[2, 0]);
    t.extend_from_slice(&[1, 0, 2, 0, 2, 0, 0, 0, b'N', 0, 0, 0]);
    t.extend_from_slice(&[2, 0, 5, 0, 3, 0, 0, 0, 56, 0, 0, 0]);
    t.extend_from_slice(&[0; 4]);
    assert_eq!(t.len(), 56);
    for (n, d) in [(49u32, 1u32), (0, 1), (0, 1)] {
        t.extend_from_slice(&n.to_le_bytes());
        t.extend_from_slice(&d.to_le_bytes());
    }
    let tags = crate::exif::read(&t);
    assert_eq!(
        tags.get("latitude").and_then(|v| v.as_f64()),
        None,
        "no longitude, no pair"
    );
    assert!(tags.contains_key("GPSLatitude"), "{tags:?}");
    assert!(metadata_summary(&t, Format::Tiff).1, "has GPS");
}
