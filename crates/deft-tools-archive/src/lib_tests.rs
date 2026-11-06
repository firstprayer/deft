use super::*;

#[test]
fn round_trip_every_format() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("src");
    std::fs::create_dir_all(src.join("sub")).unwrap();
    std::fs::write(src.join("a.txt"), "alpha").unwrap();
    std::fs::write(src.join("sub/b.txt"), "beta").unwrap();
    for (format, name) in [
        (Format::Zip, "x.zip"),
        (Format::Tar, "x.tar"),
        (Format::TarGz, "x.tar.gz"),
    ] {
        let archive = dir.path().join(name);
        assert_eq!(
            create(std::slice::from_ref(&src), &archive, format, None).unwrap(),
            2
        );
        let names: Vec<String> = list(&archive, format)
            .unwrap()
            .into_iter()
            .map(|e| e.0)
            .collect();
        assert_eq!(names, ["src/a.txt", "src/sub/b.txt"]);
        let out = dir.path().join(format!("out-{name}"));
        std::fs::create_dir_all(&out).unwrap();
        assert_eq!(
            extract(&archive, format, &out, u64::MAX / 2, None).unwrap(),
            2
        );
        assert_eq!(
            std::fs::read_to_string(out.join("src/sub/b.txt")).unwrap(),
            "beta"
        );
    }
}

#[test]
fn rejects_escaping_entries_and_bombs() {
    assert!(safe_join(Path::new("/t"), "../evil").is_none());
    assert!(safe_join(Path::new("/t"), "/etc/passwd").is_none());
    assert_eq!(
        safe_join(Path::new("/t"), "a/./b"),
        Some(PathBuf::from("/t/a/b"))
    );
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("big.txt");
    std::fs::write(&f, vec![b'x'; 1000]).unwrap();
    let a = dir.path().join("a.zip");
    create(&[f], &a, Format::Zip, None).unwrap();
    let out = dir.path().join("o");
    std::fs::create_dir_all(&out).unwrap();
    let e = extract(&a, Format::Zip, &out, 10, None).unwrap_err();
    assert_eq!(e.code, ErrorCode::LimitExceeded);
}

#[test]
fn zip_passwords() {
    let dir = tempfile::tempdir().unwrap();
    let f = dir.path().join("f.txt");
    std::fs::write(&f, b"secret").unwrap();
    let a = dir.path().join("a.zip");
    create(&[f], &a, Format::Zip, Some("pw")).unwrap();
    let out = dir.path().join("out");
    let code = |p| extract(&a, Format::Zip, &out, 1000, p).unwrap_err().code;
    assert_eq!(code(None), ErrorCode::InputEncrypted);
    assert_eq!(code(Some("no")), ErrorCode::InputEncrypted);
    extract(&a, Format::Zip, &out, 1000, Some("pw")).unwrap();
    assert_eq!(std::fs::read(out.join("f.txt")).unwrap(), b"secret");
}

#[test]
fn every_tarball_round_trips() {
    for format in [Format::TarBz2, Format::TarXz, Format::TarZst] {
        let dir = tempfile::tempdir().unwrap();
        let f = dir.path().join("f.txt");
        std::fs::write(&f, b"hello").unwrap();
        let a = dir.path().join(format!("a.{}", format.ext()));
        create(&[f], &a, format, None).unwrap();
        assert_eq!(list(&a, format).unwrap(), vec![("f.txt".to_string(), 5)]);
        let out = dir.path().join("out");
        extract(&a, format, &out, 1000, None).unwrap();
        assert_eq!(std::fs::read(out.join("f.txt")).unwrap(), b"hello");
    }
}
