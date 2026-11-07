use super::*;

#[test]
fn legacy_text_is_not_mistaken_for_broken_utf8() {
    let gbk = encoding_rs::GBK
        .encode("\u{4e2d}\u{6587}\u{6587}\u{672c}\u{5185}\u{5bb9}")
        .0;
    assert_ne!(detect_encoding(&gbk), encoding_rs::UTF_8);
    assert_eq!(detect_encoding(b"caf\xC3\xA9 \xFF"), encoding_rs::UTF_8);
    assert!(looks_binary(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR"));
    assert!(!looks_binary(b"plain"));
}
