use super::*;
use crate::{load, save};
use lopdf::dictionary;

/// A one-page document showing one image.
fn with_image(image: Stream) -> Document {
    let mut doc = Document::with_version("1.7");
    let tree = doc.new_object_id();
    let im = doc.add_object(image);
    let page = doc.add_object(dictionary! {
        "Type" => "Page", "Parent" => tree,
        "MediaBox" => vec![0.into(), 0.into(), 100.into(), 100.into()],
        "Resources" => dictionary! { "XObject" => dictionary! { "Im1" => im } },
    });
    doc.objects.insert(
        tree,
        Object::Dictionary(
            dictionary! { "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1 },
        ),
    );
    let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => tree });
    doc.trailer.set("Root", cat);
    load(&save(&mut doc).unwrap(), None).unwrap()
}

#[test]
fn a_fax_image_is_decoded() {
    // Eight black pixels in CCITT group 3.
    let image = Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => 8, "Height" => 1,
            "ColorSpace" => "DeviceGray", "BitsPerComponent" => 1,
            "Filter" => "CCITTFaxDecode",
            "DecodeParms" => dictionary! { "K" => 0, "Columns" => 8, "Rows" => 1 },
        },
        vec![0x35, 0x14],
    )
    .with_compression(false);
    let found = images(&with_image(image), &[0]);
    assert_eq!(found.skipped, 0);
    let Extracted::Pixels(img) = &found.images[0].1 else {
        panic!("decoded pixels expected");
    };
    assert_eq!((img.width(), img.height()), (8, 1));
    assert!(img.to_luma8().pixels().all(|p| p.0[0] == 0));
}

#[test]
fn a_jpeg_inside_flate_comes_out_as_the_jpeg() {
    let picture = DynamicImage::ImageRgb8(RgbImage::from_pixel(16, 16, image::Rgb([200, 30, 30])));
    let mut jpeg = Vec::new();
    picture
        .write_to(
            &mut std::io::Cursor::new(&mut jpeg),
            image::ImageFormat::Jpeg,
        )
        .unwrap();
    let image = Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => 16, "Height" => 16,
            "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
            "Filter" => vec![Object::Name(b"FlateDecode".to_vec()), Object::Name(b"DCTDecode".to_vec())],
        },
        crate::flate(&jpeg),
    )
    .with_compression(false);
    let found = images(&with_image(image), &[0]);
    let Extracted::Jpeg(bytes) = &found.images[0].1 else {
        panic!("the JPEG expected");
    };
    assert_eq!(bytes, &jpeg);
}

#[test]
fn a_black_separation_image_is_saved_as_gray() {
    let mut doc = Document::with_version("1.7");
    let space = Object::Array(vec![
        Object::Name(b"DeviceN".to_vec()),
        Object::Array(vec![Object::Name(b"Black".to_vec())]),
        Object::Name(b"DeviceCMYK".to_vec()),
        Object::Null,
    ]);
    let stream = Stream::new(
        dictionary! {
            "Type" => "XObject", "Subtype" => "Image", "Width" => 2, "Height" => 1,
            "BitsPerComponent" => 8, "ColorSpace" => space,
        },
        vec![0, 255],
    );
    let id = doc.add_object(stream);
    let s = doc.get_object(id).unwrap().as_stream().unwrap().clone();
    let img = super::decode(&doc, &s).expect("decoded").to_luma8();
    assert_eq!(img.as_raw(), &vec![255, 0]);
}
