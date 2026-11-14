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

#[test]
fn utf16_text_is_not_mp3() {
    assert_ne!(sniff(b"\xFF\xFEa\0b\0", None), Some(Format::Mp3));
    assert_eq!(sniff(b"\xFF\xFB\x90\x00", None), Some(Format::Mp3));
}

#[test]
fn data_commands_read_by_content() {
    let toml = &[Format::Toml];
    assert_eq!(
        read_as_data(None, toml, b"[[package]]\n"),
        Some(Format::Toml)
    );
    let xml = &[Format::Xml];
    assert_eq!(
        read_as_data(Some(Format::Gpx), xml, b"<?xml version=\"1.0\"?><gpx/>"),
        Some(Format::Xml)
    );
    assert_eq!(
        read_as_data(None, xml, b"<Project Sdk=\"x\">"),
        Some(Format::Xml)
    );
    // Binary content and other groups are left alone.
    assert_eq!(read_as_data(None, toml, b"\0\x01"), None);
    assert_eq!(read_as_data(None, &[Format::Csv], b"a,b"), None);
    assert_eq!(read_as_data(Some(Format::Png), xml, b"<x/>"), None);
}
