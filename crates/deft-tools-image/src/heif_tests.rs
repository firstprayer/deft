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

fn full(kind: &[u8], version: u8, body: &[u8]) -> Vec<u8> {
    make(kind, &[&[version, 0, 0, 0][..], body].concat())
}

/// A HEIC whose primary item 3 is a 2-column grid, 3x2 in all, of two 2x2
/// HEVC tiles (items 1 and 2). The grid description lives in `idat`.
fn grid_file() -> Vec<u8> {
    let infe = |id: u8, kind: &[u8]| full(b"infe", 2, &[&[0, id, 0, 0][..], kind, b"\0"].concat());
    let iinf = full(
        b"iinf",
        0,
        &[
            &[0, 3][..],
            &infe(1, b"hvc1"),
            &infe(2, b"hvc1"),
            &infe(3, b"grid"),
        ]
        .concat(),
    );
    // hvcC: 22 bytes of profile data, 4-byte lengths, one array with one NAL.
    let mut hvcc = vec![1; 21];
    hvcc.extend_from_slice(&[3, 1, 0x20, 0, 1, 0, 2, 0x40, 0x01]);
    let ipco = [
        make(b"hvcC", &hvcc),
        size(2, 2),
        size(3, 2),
        make(b"irot", &[1]),
    ]
    .concat();
    let ipma = [
        &[0u8, 0, 0, 0, 0, 0, 0, 3][..],
        &[0, 1, 2, 0x81, 2],
        &[0, 2, 2, 0x81, 2],
        &[0, 3, 2, 3, 4],
    ]
    .concat();
    let iprp = make(
        b"iprp",
        &[make(b"ipco", &ipco), make(b"ipma", &ipma)].concat(),
    );
    let iref = full(b"iref", 0, &make(b"dimg", &[0, 3, 0, 2, 0, 1, 0, 2]));
    let idat = make(b"idat", &[0, 0, 0, 1, 0, 3, 0, 2]);
    // Tile data sits at the start of `mdat`, whose offset is patched below.
    let tile = |b: u8| vec![0, 0, 0, 3, 0x26, 0x01, b];
    let loc = |id: u8, method: u8, offset: u32, len: u32| {
        [
            &[0, id, 0, method, 0, 0, 0, 1][..],
            &offset.to_be_bytes(),
            &len.to_be_bytes(),
        ]
        .concat()
    };
    let build = |mdat_at: u32| {
        let iloc = full(
            b"iloc",
            1,
            &[
                &[0x44, 0x00, 0, 3][..],
                &loc(1, 0, mdat_at, 7),
                &loc(2, 0, mdat_at + 7, 7),
                &loc(3, 1, 0, 8),
            ]
            .concat(),
        );
        let meta = full(
            b"meta",
            0,
            &[
                make(b"pitm", &[0, 0, 0, 0, 0, 3]),
                iinf.clone(),
                iloc,
                iprp.clone(),
                iref.clone(),
                idat.clone(),
            ]
            .concat(),
        );
        [make(b"ftyp", b"heic"), meta].concat()
    };
    let head = build(0);
    let at = head.len() as u32 + 8;
    [build(at), make(b"mdat", &[tile(0xAA), tile(0xBB)].concat())].concat()
}

#[test]
fn a_grid_becomes_one_stream_of_whole_tiles() {
    let bytes = grid_file();
    let p = picture(&bytes).unwrap();
    let c = &p.colour;
    assert_eq!(c.codec, Codec::Hevc);
    assert_eq!((c.tiles, c.cols, c.tile_width, c.tile_height), (2, 2, 2, 2));
    assert_eq!((c.width, c.height), (3, 2));
    // Each tile: its parameter set, then its picture data, as Annex B.
    let one = |b: u8| vec![0, 0, 0, 1, 0x40, 0x01, 0, 0, 0, 1, 0x26, 0x01, b];
    assert_eq!(c.stream, [one(0xAA), one(0xBB)].concat());
    assert_eq!(p.alpha, None);
    assert_eq!(read(&bytes).unwrap().orientation, 8);
}

#[test]
fn tiles_are_joined_and_cut_to_the_picture() {
    let coded = Coded {
        codec: Codec::Hevc,
        stream: Vec::new(),
        tiles: 2,
        cols: 2,
        tile_width: 2,
        tile_height: 2,
        width: 3,
        height: 1,
        nclx: None,
    };
    // Gray tiles: the left one 1 2 / 3 4, the right one 5 6 / 7 8.
    let joined = join(&[1, 2, 3, 4, 5, 6, 7, 8], &coded, 1).unwrap();
    assert_eq!(joined, vec![1, 2, 5]);
    assert_eq!(join(&[1, 2, 3], &coded, 1), None);
}
