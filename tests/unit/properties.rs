//! Property tests: invariants that should hold for any input.

use deft_core::value::{PageSet, SizeSpec, parse_duration, right_angle};
use deft_tools_pdf::lopdf::{Dictionary, Document, Object, Stream, dictionary};
use proptest::prelude::*;

/// A small PDF with a line of text on each page.
fn text_pdf(label: &str, pages: usize) -> Vec<u8> {
    let mut doc = Document::with_version("1.7");
    let tree = doc.new_object_id();
    let font = doc.add_object(
        dictionary! { "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica" },
    );
    let mut kids = vec![];
    for i in 0..pages {
        let text = format!("BT /F1 24 Tf 60 760 Td ({label} page {}) Tj ET", i + 1);
        let c = doc.add_object(Stream::new(Dictionary::new(), text.into_bytes()));
        let p = doc.add_object(dictionary! { "Type" => "Page", "Parent" => tree, "Contents" => c });
        kids.push(Object::Reference(p));
    }
    let n = kids.len() as i64;
    doc.objects.insert(
        tree,
        Object::Dictionary(dictionary! {
            "Type" => "Pages", "Kids" => kids, "Count" => n,
            "MediaBox" => vec![0.into(), 0.into(), 595.into(), 842.into()],
            "Resources" => dictionary! { "Font" => dictionary! { "F1" => font } },
        }),
    );
    let cat = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => tree });
    doc.trailer.set("Root", cat);
    deft_tools_pdf::save(&mut doc).expect("save")
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]

    #[test]
    fn page_ranges_stay_in_bounds(a in 1usize..30, b in 1usize..30, count in 1usize..30) {
        let set = PageSet::parse(&format!("{a}-{b}")).unwrap();
        match set.resolve(count) {
            Ok(pages) => {
                prop_assert!(pages.iter().all(|p| *p < count));
                prop_assert_eq!(pages.len(), a.abs_diff(b) + 1);
            }
            Err(_) => prop_assert!(a > count || b > count),
        }
    }

    #[test]
    fn page_parser_never_panics(s in "[0-9a-z,\\-]{0,12}") {
        if let Ok(set) = PageSet::parse(&s) {
            let _ = set.resolve(5);
        }
    }

    #[test]
    fn duration_formats_agree(h in 0u32..5, m in 0u32..60, s in 0u32..60) {
        let total = (h * 3600 + m * 60 + s) as f64;
        prop_assert_eq!(parse_duration(&format!("{h}:{m:02}:{s:02}")).unwrap(), total);
        prop_assert_eq!(parse_duration(&format!("{h}h{m}m{s}s")).unwrap(), total);
    }

    #[test]
    fn sizes_keep_the_aspect_ratio(w in 10u32..4000, h in 10u32..4000, target in 10u32..2000) {
        let (tw, th) = SizeSpec::parse(&format!("{target}x")).unwrap().target(w, h);
        prop_assert_eq!(tw, target);
        let expected = h as f64 * target as f64 / w as f64;
        prop_assert!((th as f64 - expected).abs() <= 1.0);
    }

    #[test]
    fn four_quarter_turns_are_identity(angle in prop::sample::select(vec![90.0, 180.0, 270.0, -90.0])) {
        let a = right_angle(angle).unwrap();
        prop_assert_eq!((a * 4).rem_euclid(360), 0);
    }

    #[test]
    fn split_then_merge_preserves_pages(pages in 1usize..8, every in 1usize..5) {
        let bytes = text_pdf("p", pages);
        let doc = deft_tools_pdf::load(&bytes, None).unwrap();
        let mut parts = Vec::new();
        let idx: Vec<usize> = (0..pages).collect();
        for chunk in idx.chunks(every) {
            let mut part = doc.clone();
            deft_tools_pdf::select(&mut part, chunk).unwrap();
            parts.push(deft_tools_pdf::load(&deft_tools_pdf::save(&mut part).unwrap(), None).unwrap());
        }
        let mut merged = deft_tools_pdf::merge(parts).unwrap();
        let back = deft_tools_pdf::load(&deft_tools_pdf::save(&mut merged).unwrap(), None).unwrap();
        prop_assert_eq!(deft_tools_pdf::page_ids(&back).len(), pages);
        let text = deft_tools_pdf::extract_text(&back, &[pages - 1]).unwrap();
        let wanted = format!("p page {pages}");
        prop_assert!(text.contains(&wanted));
    }

    #[test]
    fn rotation_is_cumulative(turns in 1usize..9) {
        let bytes = text_pdf("r", 1);
        let mut doc = deft_tools_pdf::load(&bytes, None).unwrap();
        for _ in 0..turns {
            deft_tools_pdf::rotate(&mut doc, &[0], 90);
        }
        let id = deft_tools_pdf::page_ids(&doc)[0];
        prop_assert_eq!(deft_tools_pdf::page_rotation(&doc, id), (turns as i64 * 90) % 360);
    }
}
