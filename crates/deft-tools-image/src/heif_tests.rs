use super::*;

fn make(kind: &[u8], body: &[u8]) -> Vec<u8> {
    let mut out = ((body.len() + 8) as u32).to_be_bytes().to_vec();
    out.extend_from_slice(kind);
    out.extend_from_slice(body);
    out
}

/// A file whose primary item 7 has the given properties. Item 1 owns property
/// 1, a size that must be ignored.
fn file(props: &[Vec<u8>]) -> Vec<u8> {
    let small = make(b"ispe", &[0, 0, 0, 0, 0, 0, 0, 9, 0, 0, 0, 9]);
    let ipco: Vec<u8> = std::iter::once(&small)
        .chain(props)
        .flatten()
        .copied()
        .collect();
    let mut ipma = vec![0, 0, 0, 0, 0, 0, 0, 2, 0, 1, 1, 1, 0, 7, props.len() as u8];
    ipma.extend((0..props.len() as u8).map(|i| i + 2));
    let iprp = [make(b"ipco", &ipco), make(b"ipma", &ipma)].concat();
    let meta = [
        vec![0; 4],
        make(b"pitm", &[0, 0, 0, 0, 0, 7]),
        make(b"iprp", &iprp),
    ]
    .concat();
    [make(b"ftyp", b"heic"), make(b"meta", &meta)].concat()
}

fn size(w: u8, h: u8) -> Vec<u8> {
    make(b"ispe", &[0, 0, 0, 0, 0, 0, 0, w, 0, 0, 0, h])
}

#[test]
fn reads_the_primary_item_only() {
    let info = read(&file(&[size(40, 30), make(b"irot", &[3])])).unwrap();
    assert_eq!((info.width, info.height, info.orientation), (40, 30, 6));
    assert_eq!(info.icc, None);
    assert!(!info.alpha);
}

#[test]
fn keeps_an_embedded_profile_but_not_a_colour_code() {
    let prof = make(b"colr", b"profICC!");
    assert_eq!(read(&file(&[prof])).unwrap().icc, Some(b"ICC!".to_vec()));
    let nclx = make(b"colr", b"nclx\0\x01\0\x0d\0\x01\x80");
    assert_eq!(read(&file(&[nclx])).unwrap().icc, None);
}

#[test]
fn turn_then_mirror_matches_exif() {
    let case = |turns: u8, axis: u8| {
        read(&file(&[make(b"irot", &[turns]), make(b"imir", &[axis])]))
            .unwrap()
            .orientation
    };
    assert_eq!(case(1, 0), 5);
    assert_eq!(case(3, 0), 7);
    assert_eq!(case(0, 1), 2);
    assert_eq!(case(0, 0), 4);
}

#[test]
fn a_mirror_listed_first_acts_on_the_other_axis() {
    let info = read(&file(&[make(b"imir", &[1]), make(b"irot", &[1])])).unwrap();
    assert_eq!(info.orientation, 5);
}

#[test]
fn other_files_and_cut_files_give_nothing() {
    assert_eq!(read(b"not a heif file"), None);
    let whole = file(&[size(40, 30)]);
    assert!(read(&whole[..whole.len() - 6]).is_none());
}
