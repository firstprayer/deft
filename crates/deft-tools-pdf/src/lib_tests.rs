use super::*;

fn text_pdf(label: &str, pages: usize) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let tree = doc.new_object_id();
    let font = doc.add_object(
        dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" },
    );
    let mut kids = vec![];
    for i in 0..pages {
        let content = format!("BT /F1 24 Tf 60 760 Td ({label} page {}) Tj ET", i + 1);
        let c = doc.add_object(Stream::new(Dictionary::new(), content.into_bytes()));
        let p = doc.add_object(dictionary! { "Type" => "Page", "Parent" => tree, "Contents" => c });
        kids.push(Object::Reference(p));
    }
    let n = kids.len() as i64;
    doc.objects.insert(
        tree,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => kids, "Count" => n,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        }),
    );
    let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => tree });
    doc.trailer.set("Root", cat);
    save(&mut doc).unwrap()
}

#[test]
fn merge_select_rotate_round_trip() {
    let a = load(&text_pdf("a", 2), None).unwrap();
    let b = load(&text_pdf("b", 3), None).unwrap();
    let mut m = merge(vec![a, b]).unwrap();
    let bytes = save(&mut m).unwrap();
    let mut m = load(&bytes, None).unwrap();
    assert_eq!(page_ids(&m).len(), 5);
    // Inherited MediaBox and Resources survive the new page tree.
    assert_eq!(display_size(&m, page_ids(&m)[4]), (595.0, 842.0));
    assert!(extract_text(&m, &[4]).unwrap().contains("b page 3"));
    select(&mut m, &[4, 0, 0]).unwrap();
    rotate(&mut m, &[0], 90);
    let bytes = save(&mut m).unwrap();
    let m = load(&bytes, None).unwrap();
    let ids = page_ids(&m);
    assert_eq!(ids.len(), 3);
    assert_eq!(page_rotation(&m, ids[0]), 90);
    assert_eq!(display_size(&m, ids[0]), (842.0, 595.0));
    assert!(extract_text(&m, &[0]).unwrap().contains("b page 3"));
}

#[test]
fn removed_pages_leave_the_file() {
    // A bookmark and a link point at page 3; keeping page 1 must drop page 3's content.
    let mut doc = load(&text_pdf("keep", 3), None).unwrap();
    let ids = page_ids(&doc);
    let big =
        doc.add_object(Stream::new(Dictionary::new(), vec![b'x'; 50_000]).with_compression(false));
    doc.get_dictionary_mut(ids[2])
        .unwrap()
        .set("Contents", Object::Reference(big));
    let outline = doc.add_object(dictionary! { "Title" => Object::string_literal("x"), "Dest" => vec![Object::Reference(ids[2])] });
    let link = dictionary! { "Subtype" => "Link", "Dest" => vec![Object::Reference(ids[2])] };
    doc.get_dictionary_mut(ids[0])
        .unwrap()
        .set("Annots", vec![Object::Dictionary(link)]);
    doc.catalog_mut()
        .unwrap()
        .set("Outlines", Object::Reference(outline));
    select(&mut doc, &[0]).unwrap();
    let bytes = save(&mut doc).unwrap();
    assert!(
        bytes.len() < 5_000,
        "removed page content is gone: {}",
        bytes.len()
    );
}

#[test]
fn stamp_and_render() {
    let mut doc = load(&text_pdf("s", 1), None).unwrap();
    let mut st = Stamper::new();
    let page = page_ids(&doc)[0];
    let text = win_ansi("DRAFT").unwrap();
    st.stamp(
        &mut doc,
        page,
        &[Mark::Text {
            bytes: text,
            size: 60.0,
            color: [200, 0, 0, 255],
            x: 150.0,
            y: 400.0,
            angle: 30.0,
        }],
        128,
    )
    .unwrap();
    let bytes = save(&mut doc).unwrap();
    let img = Renderer::new(&bytes)
        .unwrap()
        .render(0, 36.0, u64::MAX)
        .unwrap();
    assert_eq!((img.width(), img.height()), (298, 421));
    assert!(
        img.pixels().any(|p| p[0] > 200 && p[1] < 190 && p[2] < 190),
        "watermark pixels exist"
    );
    assert!(win_ansi("水印").is_err());
}

#[test]
fn images_to_pdf_pages() {
    use deft_core::format::Format;
    let img = image::DynamicImage::ImageRgb8(image::RgbImage::from_fn(200, 100, |x, _| {
        image::Rgb([x as u8, 0, 0])
    }));
    let jpg = deft_tools_image::encode(&img, Format::Jpeg, Default::default()).unwrap();
    let png = deft_tools_image::encode(&img, Format::Png, Default::default()).unwrap();
    let a = PdfImage::from_bytes(&jpg, Format::Jpeg, u64::MAX).unwrap();
    assert!(matches!(a, PdfImage::Jpeg { .. }));
    let b = PdfImage::from_bytes(&png, Format::Png, u64::MAX).unwrap();
    let mut doc = images_to_pdf(
        &[a, b],
        PageSetup {
            paper: Some((595.28, 841.89)),
            landscape: None,
            margin: 0.0,
        },
    )
    .unwrap();
    let bytes = save(&mut doc).unwrap();
    let doc = load(&bytes, None).unwrap();
    assert_eq!(page_ids(&doc).len(), 2);
    let (w, h) = display_size(&doc, page_ids(&doc)[0]);
    assert!(w > h, "landscape image gets a landscape page");
}

/// A one-page PDF holding a noisy RGB image in an ICC colour space, as a
/// Chromium print does.
fn icc_image_pdf() -> (Document, ObjectId) {
    let mut doc = load(&text_pdf("icc", 1), None).unwrap();
    let profile = doc.add_object(Stream::new(dictionary! { "N" => 3 }, vec![0; 16]));
    let space = vec![
        Object::Name(b"ICCBased".to_vec()),
        Object::Reference(profile),
    ];
    let (w, h) = (400u32, 400u32);
    let mut seed = 1u32;
    let pixels: Vec<u8> = (0..w * h * 3)
        .map(|_| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            (seed >> 24) as u8
        })
        .collect();
    let dict = dictionary! {
        "Type" => "XObject", "Subtype" => "Image", "Width" => w as i64, "Height" => h as i64,
        "ColorSpace" => space, "BitsPerComponent" => 8,
    };
    let image = doc.add_object(Stream::new(dict, pixels).with_compression(false));
    (doc, image)
}

#[test]
fn images_in_an_icc_colour_space_are_recompressed() {
    let (mut doc, image) = icc_image_pdf();
    let policy = ImagePolicy {
        dpi: 150.0,
        quality: 60,
        grayscale: false,
    };
    assert_eq!(recompress_images(&mut doc, policy), 1);
    let s = doc.get_object(image).unwrap().as_stream().unwrap();
    assert_eq!(
        s.dict.get(b"Filter").unwrap().as_name().unwrap(),
        b"DCTDecode"
    );
    assert!(
        matches!(s.dict.get(b"ColorSpace").unwrap(), Object::Array(_)),
        "the profile stays"
    );
}

#[test]
fn grayscale_replaces_an_icc_colour_space() {
    let (mut doc, image) = icc_image_pdf();
    let policy = ImagePolicy {
        dpi: 150.0,
        quality: 60,
        grayscale: true,
    };
    assert_eq!(recompress_images(&mut doc, policy), 1);
    let s = doc.get_object(image).unwrap().as_stream().unwrap();
    assert_eq!(
        s.dict.get(b"ColorSpace").unwrap().as_name().unwrap(),
        b"DeviceGray"
    );
}

/// A file whose objects sit in an object stream, with a comment before the
/// page, as qpdf writes them.
fn packed_pdf(page: &str) -> Vec<u8> {
    let members = [
        "<< /Type /Catalog /Pages 3 0 R >>\n".to_string(),
        "<< /Type /Pages /Kids [4 0 R] /Count 1 >>\n".to_string(),
        format!("% Page 1\n{page}\n"),
    ];
    let mut index = String::new();
    let mut body = String::new();
    for (n, m) in members.iter().enumerate() {
        index.push_str(&format!("{} {} ", n + 2, body.len()));
        body.push_str(m);
    }
    index.push('\n');
    let content = format!("{index}{body}");
    let mut out = b"%PDF-1.5\n".to_vec();
    let packed = out.len();
    out.extend(format!(
        "1 0 obj\n<< /Type /ObjStm /N 3 /First {} /Length {} >>\nstream\n{content}\nendstream\nendobj\n",
        index.len(),
        content.len()
    ).bytes());
    let contents = out.len();
    out.extend(b"5 0 obj\n<< /Length 37 >>\nstream\nBT /F1 24 Tf 72 720 Td (Potato) Tj ET\nendstream\nendobj\n");
    let xref = out.len();
    let mut rows = vec![0u8, 0, 0, 0];
    let normal = |at: usize| [1, (at >> 8) as u8, at as u8, 0];
    rows.extend(normal(packed));
    for i in 0..3 {
        rows.extend([2, 0, 1, i]);
    }
    rows.extend(normal(contents));
    rows.extend(normal(xref));
    out.extend(
        format!(
            "6 0 obj\n<< /Type /XRef /Size 7 /W [1 2 1] /Root 2 0 R /Length {} >>\nstream\n",
            rows.len()
        )
        .bytes(),
    );
    out.extend(rows);
    out.extend(format!("\nendstream\nendobj\nstartxref\n{xref}\n%%EOF\n").bytes());
    out
}

#[test]
fn a_page_after_a_comment_in_an_object_stream_is_kept() {
    let bytes =
        packed_pdf("<< /Type /Page /Parent 3 0 R /MediaBox [0 0 200 200] /Contents 5 0 R >>");
    let mut doc = load(&bytes, None).unwrap();
    let ids = page_ids(&doc);
    assert_eq!(ids.len(), 1);
    assert!(doc.get_dictionary(ids[0]).unwrap().has(b"Contents"));
    let doc = load(&save(&mut doc).unwrap(), None).unwrap();
    assert_eq!(display_size(&doc, page_ids(&doc)[0]), (200.0, 200.0));
}

#[test]
fn a_page_that_cannot_be_read_is_refused() {
    let e = load(&packed_pdf("(not a page)"), None).unwrap_err();
    assert_eq!(e.code, ErrorCode::InputCorrupt);
    assert!(e.message.contains("page 1"), "{}", e.message);
}
