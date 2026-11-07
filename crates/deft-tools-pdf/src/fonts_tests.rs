use super::*;
use lopdf::dictionary;

fn mapped(doc: &Document, font: ObjectId) -> CMap {
    let id = doc
        .get_dictionary(font)
        .unwrap()
        .get(b"ToUnicode")
        .unwrap()
        .as_reference()
        .unwrap();
    let data = doc
        .get_object(id)
        .unwrap()
        .as_stream()
        .unwrap()
        .content
        .clone();
    CMap::parse(&data, load_embedded).unwrap()
}

fn text(map: &CMap, code: u32) -> Option<String> {
    Some(match map.lookup_bf_string(code)? {
        BfString::Char(c) => c.to_string(),
        BfString::String(s) => s,
    })
}

#[test]
fn an_empty_mapping_does_not_end_the_map() {
    let mut doc = Document::with_version("1.7");
    let map = b"/CIDInit /ProcSet findresource begin 12 dict begin begincmap\n\
        1 begincodespacerange <0000> <FFFF> endcodespacerange\n\
        2 beginbfchar\n<03f2> <>\n<03a3> <062D0628>\nendbfchar\nendcmap end end\n";
    let stream = doc.add_object(Stream::new(Dictionary::new(), map.to_vec()));
    let font = doc
        .add_object(dictionary! { "Type" => "Font", "Subtype" => "Type0", "ToUnicode" => stream });
    add_text_maps(&mut doc);
    let cmap = mapped(&doc, font);
    assert_eq!(text(&cmap, 0x03a3).as_deref(), Some("حب"));
    assert_eq!(text(&cmap, 0x03f2).as_deref(), Some("\u{200B}"));
}

#[test]
fn glyphs_named_after_their_code_read_as_that_code() {
    let mut doc = Document::with_version("1.7");
    let names: Vec<Object> = std::iter::once(Object::Integer(65))
        .chain(["g65", "g66", "g67", "g68"].map(|n| Object::Name(n.as_bytes().to_vec())))
        .chain([Object::Integer(233), Object::Name(b"a233".to_vec())])
        .collect();
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type3",
        "Encoding" => dictionary! { "Differences" => names },
    });
    add_text_maps(&mut doc);
    let cmap = mapped(&doc, font);
    assert_eq!(text(&cmap, 66).as_deref(), Some("B"));
    assert_eq!(text(&cmap, 233).as_deref(), Some("é"));
}

#[test]
fn a_font_named_in_gbk_passes_its_high_bytes_on() {
    let mut doc = Document::with_version("1.7");
    // 宋体 in GBK.
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "TrueType",
        "BaseFont" => Object::Name(vec![0xCB, 0xCE, 0xCC, 0xE5]),
        "Encoding" => "WinAnsiEncoding",
    });
    add_text_maps(&mut doc);
    let cmap = mapped(&doc, font);
    assert_eq!(text(&cmap, 0x41).as_deref(), Some("A"));
    let high = char::from_u32(GBK_BYTES + 0xC4 - 0x80).unwrap().to_string();
    assert_eq!(text(&cmap, 0xC4), Some(high));
}

#[test]
fn a_japanese_cid_font_without_a_map_gets_its_collection() {
    let mut doc = Document::with_version("1.7");
    let info = dictionary! {
        "Registry" => Object::string_literal("Adobe"),
        "Ordering" => Object::string_literal("Japan1"),
        "Supplement" => 6,
    };
    let cid = doc.add_object(
        dictionary! { "Type" => "Font", "Subtype" => "CIDFontType2", "CIDSystemInfo" => info },
    );
    let font = doc.add_object(dictionary! {
        "Type" => "Font", "Subtype" => "Type0", "Encoding" => "Identity-V",
        "DescendantFonts" => vec![Object::Reference(cid)],
    });
    add_text_maps(&mut doc);
    let cmap = mapped(&doc, font);
    // CID 843 is あ in Adobe-Japan1.
    assert_eq!(text(&cmap, 843).as_deref(), Some("あ"));
}
