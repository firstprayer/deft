use super::*;

#[test]
fn percent_escapes_decode() {
    assert_eq!(percent_decode("my%20photo.png"), "my photo.png");
    assert_eq!(percent_decode("caf%C3%A9"), "café");
    assert_eq!(percent_decode("100%.png"), "100%.png");
    assert_eq!(percent_decode("%zz%2"), "%zz%2");
}

#[test]
fn pages() {
    let p = PageSet::parse("1-3,5,8-").unwrap();
    assert_eq!(p.resolve(10).unwrap(), vec![0, 1, 2, 4, 7, 8, 9]);
    assert_eq!(PageSet::parse("last").unwrap().resolve(4).unwrap(), vec![3]);
    assert_eq!(PageSet::parse("-2").unwrap().resolve(4).unwrap(), vec![2]);
    assert_eq!(
        PageSet::parse("odd").unwrap().resolve(5).unwrap(),
        vec![0, 2, 4]
    );
    assert_eq!(
        PageSet::parse("even").unwrap().resolve(5).unwrap(),
        vec![1, 3]
    );
    assert_eq!(
        PageSet::parse("3-1").unwrap().resolve(3).unwrap(),
        vec![2, 1, 0]
    );
    assert_eq!(
        PageSet::parse("2-last").unwrap().resolve(3).unwrap(),
        vec![1, 2]
    );
    assert!(PageSet::parse("0").is_err());
    assert!(PageSet::parse("5").unwrap().resolve(3).is_err());
    assert!(PageSet::parse("a-b").is_err());
}

#[test]
fn last_minus_n_counts_back_from_the_end() {
    let pages = |s: &str| PageSet::parse(s).unwrap().resolve(14).unwrap();
    // As people read it: the last three pages, not 14 down to 2.
    assert_eq!(pages("last-2"), vec![11, 12, 13]);
    assert_eq!(pages("end-1"), vec![12, 13]);
    assert_eq!(pages("2-last-1"), (1..13).collect::<Vec<_>>());
    assert_eq!(pages("last-3-last-2"), vec![10, 11]);
    // The last two pages as a range of negative numbers.
    assert_eq!(pages("-2-"), vec![12, 13]);
    assert_eq!(pages("-2--1"), vec![12, 13]);
    assert_eq!(pages("-3"), vec![11]);
    assert_eq!(pages("3-1"), vec![2, 1, 0]);
    assert!(PageSet::parse("last-x").is_err());
    let e = PageSet::parse("-2-x").unwrap_err();
    assert!(e.contains("-2-x"), "{e}");
    assert_eq!(PageSet::parse("-0").unwrap_err(), "page -0 does not exist");
    assert_eq!(PageSet::parse("0-3").unwrap_err(), "pages start at 1");
    assert!(PageSet::parse("last-20").unwrap().resolve(14).is_err());
}

#[test]
fn durations() {
    assert_eq!(parse_duration("90").unwrap(), 90.0);
    assert_eq!(parse_duration("1:30").unwrap(), 90.0);
    assert_eq!(parse_duration("01:02:03.5").unwrap(), 3723.5);
    assert_eq!(parse_duration("1m30s").unwrap(), 90.0);
    assert_eq!(parse_duration("500ms").unwrap(), 0.5);
    assert!(parse_duration("abc").is_err());
    assert_eq!(format_duration(3723.5), "1:02:03.500");
    assert_eq!(format_duration(65.0), "1:05");
}

#[test]
fn sizes() {
    let s = SizeSpec::parse("800x").unwrap();
    assert_eq!(s.target(1600, 1200), (800, 600));
    assert_eq!(
        SizeSpec::parse("x300").unwrap().target(1600, 1200),
        (400, 300)
    );
    assert_eq!(
        SizeSpec::parse("50%").unwrap().target(1600, 1200),
        (800, 600)
    );
    assert!(SizeSpec::parse("1920x1080").unwrap().is_box());
    assert!(SizeSpec::parse("x").is_err());
    assert!(SizeSpec::parse("0x10").is_err());
}

#[test]
fn lengths_bytes_colors_positions() {
    assert!((parse_length("25.4mm").unwrap() - 72.0).abs() < 1e-9);
    assert_eq!(parse_length("1in").unwrap(), 72.0);
    assert_eq!(parse_bytes("2MB").unwrap(), 2 * 1048576);
    assert_eq!(parse_bytes("1.5kb").unwrap(), 1536);
    assert_eq!(parse_bytes("100").unwrap(), 100);
    assert_eq!(format_bytes(1536), "2KB");
    assert_eq!(parse_color("#f00").unwrap(), [255, 0, 0, 255]);
    assert_eq!(parse_color("#00000080").unwrap(), [0, 0, 0, 128]);
    assert!(parse_color("#12").is_err());
    assert_eq!(
        Position::parse("bottom-right").unwrap(),
        Position { x: 1.0, y: 1.0 }
    );
    assert_eq!(Position::parse("Top Center").unwrap().name(), "top");
    assert!(Position::parse("north").is_err());
}

#[test]
fn text_keeps_its_spaces_and_the_rest_are_trimmed() {
    assert_eq!(
        Kind::Text.parse("find", " ").unwrap(),
        Value::Text(" ".into())
    );
    assert_eq!(
        Kind::Int { min: 0, max: 9 }.parse("n", " 3 ").unwrap(),
        Value::Int(3)
    );
}
