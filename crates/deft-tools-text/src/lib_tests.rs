use super::*;

const SAMPLE: &str = "# Title *here*\n\nSome **bold** text, a [link](https://example.com) and `code`.\n\n- one\n- two\n  - nested\n\n1. first\n2. second\n\n> quoted\n\n| a | b |\n|---|:-:|\n| 1 | 2 |\n\n```rust\nfn main() {}\n```\n\n---\n\nSpecial chars: # $ * _ < > @ [x] ~ 50% a/b // c - d\n";

#[test]
fn html_page() {
    let html = to_html(SAMPLE, true, None);
    assert!(html.contains("<title>Title here</title>"));
    assert!(html.contains("<strong>bold</strong>"));
    assert!(html.contains("<table>"));
    assert!(!to_html("x", false, None).contains("<html"));
}

#[test]
fn pdf_compiles_all_elements() {
    let out = to_pdf(SAMPLE, None, &PdfStyle::default()).unwrap();
    assert!(out.bytes.starts_with(b"%PDF"));
    assert_eq!(out.pages, 1);
    assert!(out.warnings.is_empty());
}

#[test]
fn missing_image_warns_instead_of_failing() {
    let out = to_pdf(
        "![x](nope.png)\n\n<div>raw</div>\n",
        None,
        &PdfStyle::default(),
    )
    .unwrap();
    assert_eq!(out.warnings.len(), 2, "{:?}", out.warnings);
}

#[test]
fn inline_elements_followed_by_brackets_stay_text() {
    // Found on a real README: typst read "(PDF)" as call arguments.
    let md = "[a](https://x.y)(PDF，中文) **b**(c) *d*[e] `f`(g) ~~h~~(i).\n";
    let out = to_pdf(md, None, &PdfStyle::default()).unwrap();
    assert_eq!(out.pages, 1);
}

#[test]
fn output_is_reproducible() {
    let a = to_pdf(SAMPLE, None, &PdfStyle::default()).unwrap();
    let b = to_pdf(SAMPLE, None, &PdfStyle::default()).unwrap();
    assert_eq!(a.bytes, b.bytes);
}

#[test]
fn long_text_gets_places_to_break() {
    let url = format!("https://example.com/{}", "a".repeat(100));
    let out = breakable(&url);
    assert_eq!(out.matches('\u{200B}').count(), 3);
    assert_eq!(out.replace('\u{200B}', ""), url);
    let plain = "short words stay as they are";
    assert_eq!(breakable(plain), plain);
}

#[test]
fn headings_get_the_anchors_toc_links_to() {
    let md = "# Table of Contents\n\n[top](#table-of-contents) and Cell<T> where T: Ord, <em>kept</em>\n\n## Table of Contents\n";
    let html = to_html(md, false, None);
    assert!(html.contains("<h1 id=\"table-of-contents\">"), "{html}");
    assert!(html.contains("<h2 id=\"table-of-contents-1\">"), "{html}");
    assert!(html.contains("Cell&lt;T&gt; where"), "{html}");
    assert!(html.contains("<em>kept</em>"), "{html}");
    assert_eq!(
        heading_ids(md),
        ["table-of-contents", "table-of-contents-1"]
    );
}

#[test]
fn pdf_survives_empty_links_and_unknown_pictures() {
    let dir = std::env::temp_dir().join(format!("deft-text-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("pic.emf"), b"\x01\x00\x00\x00 EMF").unwrap();
    let md = "[nowhere]() and ![a chart](pic.emf)\n\n<table><tr><th colspan=\"3\">Title over all</th></tr><tr><td rowspan=\"2\">a</td><td>b</td><td>:face_with_spiral_eyes:</td></tr><tr><td>c</td><td>d</td></tr></table>\n";
    let out = to_pdf(md, Some(&dir), &PdfStyle::default()).unwrap();
    assert!(out.bytes.starts_with(b"%PDF"));
    assert!(
        out.warnings
            .iter()
            .any(|w| w.contains("EMF pictures cannot be drawn")),
        "{:?}",
        out.warnings
    );
    assert!(
        !out.warnings.iter().any(|w| w.contains("raw HTML")),
        "{:?}",
        out.warnings
    );
    let _ = std::fs::remove_dir_all(&dir);
}
