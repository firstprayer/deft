use super::*;

fn write(dir: &Path, name: &str, data: &[u8]) -> PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, data).unwrap();
    p
}

#[test]
fn known_digests() {
    let d = tempfile::tempdir().unwrap();
    let p = write(d.path(), "a", b"abc");
    let h = |a| hash_file(&p, a).unwrap();
    assert_eq!(h(Algorithm::Md5), "900150983cd24fb0d6963f7d28e17f72");
    assert_eq!(
        h(Algorithm::Sha1),
        "a9993e364706816aba3e25717850c26c9cd0d89d"
    );
    assert_eq!(
        h(Algorithm::Sha256),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(h(Algorithm::Crc32), "352441c2");
    assert!(h(Algorithm::Sha512).starts_with("ddaf35a193617aba"));
    assert!(h(Algorithm::Blake3).starts_with("6437b3ac38465133"));
}

#[test]
fn duplicates_need_the_same_content() {
    let d = tempfile::tempdir().unwrap();
    let a = write(d.path(), "a", b"same");
    let b = write(d.path(), "b", b"same");
    let c = write(d.path(), "c", b"diff");
    write(d.path(), "empty1", b"");
    write(d.path(), "empty2", b"");
    let groups = duplicates(&walk(&[d.path().to_path_buf()]).unwrap()).unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].files, vec![a, b]);
    assert!(!groups[0].files.contains(&c));
}

#[test]
fn rename_templates_and_swaps() {
    let d = tempfile::tempdir().unwrap();
    let a = write(d.path(), "a.txt", b"A");
    let b = write(d.path(), "b.txt", b"B");
    let rule = RenameRule {
        name: Some("{parent}-{n:03}.{ext}".into()),
        start: 7,
        ..Default::default()
    };
    let plan = rename_plan(std::slice::from_ref(&a), &rule).unwrap();
    let parent = d.path().file_name().unwrap().to_string_lossy().to_string();
    assert_eq!(plan[0].1, d.path().join(format!("{parent}-007.txt")));

    let rule = RenameRule {
        find: Some(Regex::new("^a$").unwrap()),
        replace: "b".into(),
        ..Default::default()
    };
    assert_eq!(
        rename_plan(std::slice::from_ref(&a), &rule)
            .unwrap_err()
            .code,
        ErrorCode::OutputExists
    );
    let swap = vec![(a.clone(), b.clone()), (b.clone(), a.clone())];
    apply_renames(&swap).unwrap();
    assert_eq!(std::fs::read(&a).unwrap(), b"B");
    assert_eq!(std::fs::read(&b).unwrap(), b"A");
}

#[test]
fn base64_is_forgiving_when_decoding() {
    assert_eq!(base64_encode(b"hi?"), "aGk/");
    for text in ["aGk/", "aGk_", "aG\nk/\n", "data:text/plain;base64,aGk/"] {
        assert_eq!(base64_decode(text).unwrap(), b"hi?");
    }
    assert_eq!(base64_decode("aGk").unwrap(), b"hi");
    assert!(base64_decode("***").is_err());
}

#[test]
fn encodings() {
    let latin = b"caf\xE9 cr\xE8me br\xFBl\xE9e et d\xE9j\xE0 vu, voil\xE0 tr\xE8s bien";
    assert_ne!(detect_encoding(latin), encoding_rs::UTF_8);
    let (text, bad) = decode_text(latin, find_encoding("latin1").unwrap());
    assert!(text.starts_with("café") && !bad);
    let (bytes, lost) = encode_text("é", find_encoding("utf8").unwrap(), true);
    assert_eq!(bytes, b"\xEF\xBB\xBF\xC3\xA9");
    assert!(!lost);
    let (_, lost) = encode_text("漢", find_encoding("latin1").unwrap(), false);
    assert!(lost);
    let (bytes, _) = encode_text("a", find_encoding("utf-16").unwrap(), true);
    assert_eq!(bytes, [0xFF, 0xFE, b'a', 0]);
    assert_eq!(set_line_endings("a\r\nb\rc\n", true), "a\r\nb\r\nc\r\n");
    assert_eq!(set_line_endings("a\r\nb\rc\n", false), "a\nb\nc\n");
}
