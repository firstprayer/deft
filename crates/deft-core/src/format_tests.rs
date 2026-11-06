use super::*;

#[test]
fn extensions_round_trip() {
    for f in Format::ALL {
        assert_eq!(Format::from_name(f.ext()), Some(*f), "{f:?}");
    }
    assert_eq!(Format::from_name("JPEG"), Some(Format::Jpeg));
    assert_eq!(
        Format::from_path(Path::new("a/b.tar.gz")),
        Some(Format::TarGz)
    );
}

#[test]
fn sniffs_signatures() {
    assert_eq!(sniff(b"%PDF-1.7\n", None), Some(Format::Pdf));
    assert_eq!(sniff(b"\x89PNG\r\n\x1a\n....", None), Some(Format::Png));
    assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0], None), Some(Format::Jpeg));
    assert_eq!(sniff(b"RIFF\0\0\0\0WEBPVP8 ", None), Some(Format::Webp));
    assert_eq!(sniff(b"\0\0\0\x18ftypisom", None), Some(Format::Mp4));
    assert_eq!(sniff(b"\0\0\0\x18ftypheic", None), Some(Format::Heic));
    // Extension says pdf, content says png: content wins.
    assert_eq!(
        sniff(b"\x89PNG\r\n\x1a\n", Some(Path::new("x.pdf"))),
        Some(Format::Png)
    );
    assert_eq!(
        sniff(b"# Title\n", Some(Path::new("notes.md"))),
        Some(Format::Md)
    );
    assert_eq!(sniff(b"<svg xmlns=", None), Some(Format::Svg));
}
