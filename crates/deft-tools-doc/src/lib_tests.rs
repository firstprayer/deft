use super::*;

const MD: &str = "# Title\n\nSome **bold** and *it* text with a [link](https://a.example).\n\n- one\n- two\n\n1. first\n\n> quoted\n\n```\ncode\n```\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n";

fn sample() -> Document {
    Document {
        title: Some("T".into()),
        blocks: markdown::parse(MD),
        ..Default::default()
    }
}

#[test]
fn markdown_round_trip() {
    let doc = sample();
    let out = markdown::render(&doc);
    assert_eq!(markdown::parse(&out), doc.blocks);
    assert!(out.contains("**bold**"));
    assert!(out.contains("[link](https://a.example)"));
}

#[test]
fn docx_round_trip_keeps_structure() {
    let doc = sample();
    let bytes = docx::write(&doc).unwrap();
    let back = docx::read(&bytes).unwrap();
    assert_eq!(back.title.as_deref(), Some("T"));
    let md = markdown::render(&back);
    assert!(md.contains("# Title"), "{md}");
    assert!(md.contains("**bold**"), "{md}");
    assert!(md.contains("[link](https://a.example)"), "{md}");
    assert!(md.contains("- one"), "{md}");
    assert!(md.contains("1. first"), "{md}");
    assert!(md.contains("| 1 | 2 |"), "{md}");
}

#[test]
fn docx_replace_spans_runs() {
    let doc = docx::write(&Document {
        blocks: vec![Block::Paragraph(vec![
            Span::plain("Hello Wo"),
            Span::plain("rld & more"),
        ])],
        ..Default::default()
    })
    .unwrap();
    let (bytes, n) = docx::replace(&doc, "World", "Earth").unwrap();
    assert_eq!(n, 1);
    assert_eq!(
        docx::read(&bytes).unwrap().text().trim(),
        "Hello Earth & more"
    );
}

#[test]
fn rtf_reads_text_styles_and_escapes() {
    let src = br"{\rtf1\ansi{\fonttbl{\f0 Arial;}}\f0 Plain \b bold\b0  caf\'e9 \u8364?\par Next}";
    let doc = rtf::read(src);
    assert_eq!(doc.blocks.len(), 2);
    let text = doc.text();
    assert!(text.contains("Plain bold café €"), "{text}");
    assert!(!text.contains("Arial"));
    let Block::Paragraph(spans) = &doc.blocks[0] else {
        panic!()
    };
    assert!(spans.iter().any(|s| s.bold && s.text.contains("bold")));
}

#[test]
fn html_conversion_drops_scripts() {
    let md = html::to_markdown(
        "<html><head><title>T</title><style>p{}</style></head><body><h1>Hi</h1><script>x()</script><p>a <b>b</b></p></body></html>",
    )
    .unwrap();
    assert!(md.contains("# Hi"));
    assert!(md.contains("**b**"));
    assert!(!md.contains("x()"));
    assert_eq!(html::title("<title> T </title>").as_deref(), Some("T"));
}

#[test]
fn epub_round_trip() {
    let book = epub::NewBook {
        title: "A & B",
        author: Some("Ada"),
        language: "en",
        chapters: vec![("One".into(), "<h1>One</h1><p>Text</p>".into())],
        images: vec![],
    };
    let bytes = epub::write(&book).unwrap();
    let back = epub::read(&bytes).unwrap();
    assert_eq!(back.title.as_deref(), Some("A & B"));
    assert_eq!(back.author.as_deref(), Some("Ada"));
    assert_eq!(back.chapters.len(), 1);
    assert!(back.markdown().unwrap().contains("# One"));
}

#[test]
fn main_content_drops_site_chrome() {
    let body = "x".repeat(300);
    let page = format!(
        "<html><body><nav><a href=x>Home</a></nav><MAIN class=a><h1>T</h1><aside>ad</aside><main-x></main-x><p>{body}</p></MAIN><footer>f</footer></body></html>"
    );
    let inner = html::main_content(&page);
    assert!(inner.contains("<h1>T</h1>") && !inner.contains("Home"));
    assert!(!inner.contains("ad") && !inner.contains("footer"));
    let plain = "<body><header>h</header><p>text</p></body>";
    assert_eq!(html::main_content(plain), "<p>text</p>");
}

#[test]
fn slides_round_trip() {
    let blocks = markdown::parse("# One\n\n- a\n- b\n\n## Two\n\nText & more\n\n---\n\nNo title\n");
    let slides = pptx::slides_from(&blocks);
    assert_eq!(slides.len(), 3);
    let bytes = pptx::write(&slides, &[]).unwrap();
    let (back, _) = pptx::read(&bytes).unwrap();
    assert_eq!(back.len(), 3);
    assert_eq!(back[0].title.as_deref(), Some("One"));
    assert_eq!(back[0].body.len(), 2);
    assert_eq!(back[1].body[0][0].text, "Text & more");
}

#[test]
fn nested_lists_stay_nested() {
    let blocks = markdown::parse("- one\n- two\n  - inner\n    - deep\n- three\n");
    let Block::List { items, .. } = &blocks[0] else {
        panic!("{blocks:?}")
    };
    assert_eq!(items.len(), 3);
    assert!(matches!(items[1][0], Block::Paragraph(_)), "{:?}", items[1]);
    assert!(matches!(items[1][1], Block::List { .. }), "{:?}", items[1]);
    let doc = Document {
        blocks: blocks.clone(),
        ..Default::default()
    };
    assert_eq!(markdown::parse(&markdown::render(&doc)), blocks);

    let slides = pptx::slides_from(&markdown::parse(
        "# T\n\n- one\n  - inner\n    - deep\n- two\n",
    ));
    let got: Vec<(usize, &str)> = slides[0]
        .bullets
        .iter()
        .map(|(l, t)| (*l, t.as_str()))
        .collect();
    assert_eq!(got, [(0, "one"), (1, "inner"), (2, "deep"), (0, "two")]);
}
