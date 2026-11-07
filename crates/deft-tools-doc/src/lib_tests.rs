use super::*;

const MD: &str = "# Title\n\nSome **bold** and *it* text with a [link](https://a.example).\n\n- one\n- two\n\n1. first\n\n> quoted\n\n```\ncode\n```\n\n| a | b |\n| --- | --- |\n| 1 | 2 |\n";

fn sample() -> Document {
    Document {
        title: Some("T".into()),
        blocks: markdown::parse(MD),
        ..Default::default()
    }
}

/// A Word file from raw parts, for structures `docx::write` never makes.
fn word(body: &str, parts: &[(&str, &str)]) -> Vec<u8> {
    use std::io::Write;
    const NS: &str = "xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:mc=\"http://schemas.openxmlformats.org/markup-compatibility/2006\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"";
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    let document = format!("<w:document {NS}><w:body>{body}</w:body></w:document>");
    let notes: Vec<(String, String)> = parts
        .iter()
        .map(|(name, xml)| {
            let root = if name.contains("foot") {
                "footnotes"
            } else {
                "endnotes"
            };
            (name.to_string(), format!("<w:{root} {NS}>{xml}</w:{root}>"))
        })
        .collect();
    zip.start_file("word/document.xml", options).unwrap();
    zip.write_all(document.as_bytes()).unwrap();
    for (name, xml) in &notes {
        zip.start_file(name.as_str(), options).unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn run(text: &str) -> String {
    format!("<w:r><w:t xml:space=\"preserve\">{text}</w:t></w:r>")
}

fn para(text: &str) -> String {
    format!("<w:p>{}</w:p>", run(text))
}

/// A text box as Word writes it: once for new readers, once for old ones.
fn text_box(text: &str) -> String {
    let inner = format!("<w:txbxContent>{}</w:txbxContent>", para(text));
    format!(
        "<w:r><mc:AlternateContent><mc:Choice Requires=\"wps\"><w:drawing>{inner}</w:drawing></mc:Choice><mc:Fallback><w:pict>{inner}</w:pict></mc:Fallback></mc:AlternateContent></w:r>"
    )
}

#[test]
fn word_text_boxes_are_read_once() {
    let body = format!(
        "<w:p>{}{}{}</w:p>",
        run("before "),
        text_box("boxed"),
        run("after")
    );
    let text = docx::read(&word(&body, &[])).unwrap().text();
    assert_eq!(text.matches("boxed").count(), 1, "{text}");
    assert!(text.contains("before after"), "{text}");
}

#[test]
fn word_tables_keep_nested_and_controlled_text() {
    let nested = format!("<w:tbl><w:tr><w:tc>{}</w:tc></w:tr></w:tbl>", para("inner"));
    let control = format!(
        "<w:sdt><w:sdtContent>{}</w:sdtContent></w:sdt>",
        para("control")
    );
    let table = format!(
        "<w:tbl><w:tr><w:tc>{}{nested}<w:p/></w:tc><w:tc>{control}</w:tc></w:tr></w:tbl>",
        para("outer")
    );
    let wrapped = format!(
        "<w:sdt><w:sdtContent><w:tbl><w:tr><w:tc>{}</w:tc></w:tr></w:tbl></w:sdtContent></w:sdt>",
        para("wrapped")
    );
    let doc = docx::read(&word(&format!("{table}{wrapped}"), &[])).unwrap();
    let Block::Table(rows) = &doc.blocks[0] else {
        panic!("{:?}", doc.blocks)
    };
    let cell = |i: usize| -> String { rows[0][i].iter().map(|s| s.text.as_str()).collect() };
    assert_eq!(cell(0), "outer inner");
    assert_eq!(cell(1), "control");
    assert!(doc.text().contains("wrapped"), "{}", doc.text());
}

#[test]
fn word_notes_follow_the_text_in_reference_order() {
    let body = format!(
        "<w:p>{}<w:r><w:endnoteReference w:id=\"2\"/></w:r>{}<w:r><w:footnoteReference w:id=\"7\"/></w:r></w:p>",
        run("One"),
        run(" two")
    );
    let foot = format!(
        "<w:footnote w:type=\"separator\" w:id=\"0\">{}</w:footnote><w:footnote w:id=\"7\">{}</w:footnote>",
        para("rule"),
        para("Foot text")
    );
    let end = format!("<w:endnote w:id=\"2\">{}</w:endnote>", para("End text"));
    let doc = docx::read(&word(
        &body,
        &[("word/footnotes.xml", &foot), ("word/endnotes.xml", &end)],
    ))
    .unwrap();
    let text = doc.text();
    assert!(text.contains("One[1] two[2]"), "{text}");
    assert!(text.contains("[1]: End text"), "{text}");
    assert!(text.contains("[2]: Foot text"), "{text}");
    assert!(!text.contains("rule"), "{text}");
    let md = markdown::render(&doc);
    assert!(md.contains("One[^1] two[^2]"), "{md}");
    assert!(md.contains("[^2]: Foot text"), "{md}");
}

#[test]
fn word_note_text_starts_clean() {
    let body = "<w:p><w:r><w:footnoteReference w:id=\"1\"/></w:r></w:p>";
    let foot = format!(
        "<w:footnote w:id=\"1\">{}</w:footnote>",
        para(" Spaced note")
    );
    let doc = docx::read(&word(body, &[("word/footnotes.xml", &foot)])).unwrap();
    assert!(doc.text().contains("[1]: Spaced note"), "{}", doc.text());
}

#[test]
fn word_list_levels_nest() {
    let item = |level: u8, text: &str| {
        format!(
            "<w:p><w:pPr><w:numPr><w:ilvl w:val=\"{level}\"/><w:numId w:val=\"1\"/></w:numPr></w:pPr>{}</w:p>",
            run(text)
        )
    };
    let body = [
        item(0, "top"),
        item(1, "mid"),
        item(2, "deep"),
        item(0, "next"),
    ]
    .concat();
    let md = markdown::render(&docx::read(&word(&body, &[])).unwrap());
    assert_eq!(md, "- top\n  - mid\n    - deep\n- next\n");
}

#[test]
fn word_replace_reaches_notes_and_text_after_a_box() {
    let body = format!(
        "<w:p>{}{}{}</w:p>",
        run("Acme a "),
        text_box("Acme b"),
        run(" Acme c")
    );
    let foot = format!("<w:footnote w:id=\"1\">{}</w:footnote>", para("Acme d"));
    let end = format!("<w:endnote w:id=\"1\">{}</w:endnote>", para("Acme e"));
    let file = word(
        &body,
        &[("word/footnotes.xml", &foot), ("word/endnotes.xml", &end)],
    );
    // The box is written twice, so five places hold six matches.
    let (bytes, n) = docx::replace(&file, "Acme", "Zed").unwrap();
    assert_eq!(n, 6);
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    for name in [
        "word/document.xml",
        "word/footnotes.xml",
        "word/endnotes.xml",
    ] {
        let mut xml = String::new();
        std::io::Read::read_to_string(&mut zip.by_name(name).unwrap(), &mut xml).unwrap();
        assert!(!xml.contains("Acme"), "{name}: {xml}");
        assert!(xml.contains("Zed"), "{name}: {xml}");
    }
}

#[test]
fn word_writer_stores_a_repeated_picture_once() {
    let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01".to_vec();
    let pic = || Span {
        image: Some("dot.png".into()),
        ..Default::default()
    };
    let doc = Document {
        blocks: vec![Block::Paragraph(vec![pic()]), Block::Paragraph(vec![pic()])],
        images: vec![Image {
            name: "dot.png".into(),
            bytes: png,
        }],
        ..Default::default()
    };
    let bytes = docx::write(&doc).unwrap();
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
    assert!(zip.by_name("word/media/dot.png").is_ok());
    let mut xml = String::new();
    std::io::Read::read_to_string(&mut zip.by_name("word/document.xml").unwrap(), &mut xml)
        .unwrap();
    assert_eq!(xml.matches("<w:drawing>").count(), 2);
}

#[test]
fn markdown_notes_become_markers_not_literal_syntax() {
    let blocks = markdown::parse("A claim.[^1]\n\n[^1]: The source.\n");
    let doc = Document {
        blocks,
        ..Default::default()
    };
    let text = doc.text();
    assert!(text.contains("A claim.[1]"), "{text}");
    assert!(text.contains("[1]: The source."), "{text}");
    assert!(!text.contains("[^"), "{text}");
    assert_eq!(
        markdown::render(&doc),
        "A claim.[^1]\n\n[^1]: The source.\n"
    );
}

#[test]
fn rtf_joins_surrogate_pairs() {
    let doc = rtf::read(br"{\rtf1\ansi Smile \u55357?\u56832? and \u-10179?\u-8704? end}");
    assert_eq!(doc.text().trim(), "Smile \u{1F600} and \u{1F600} end");
    let lone = rtf::read(br"{\rtf1\ansi a\u8364?b}");
    assert_eq!(lone.text().trim(), "a€b");
}

#[test]
fn struck_text_stays_struck() {
    let blocks = markdown::parse("Keep ~~gone~~ this\n");
    let doc = Document {
        blocks,
        ..Default::default()
    };
    assert_eq!(markdown::render(&doc), "Keep ~~gone~~ this\n");
    let back = docx::read(&docx::write(&doc).unwrap()).unwrap();
    assert_eq!(markdown::render(&back), "Keep ~~gone~~ this\n");
}

#[test]
fn links_between_chapters_become_text() {
    let book = epub::Book {
        chapters: vec![
            "<p><a href=\"ch-2.xhtml#x\">Letter 2</a> and <a href=\"https://a.example/p.html\">web</a></p><img src=\"../Images/a.png\" alt=\"pic\"/>".to_string(),
        ],
        ..Default::default()
    };
    let md = book.markdown().unwrap();
    assert!(
        md.contains("Letter 2 and [web](https://a.example/p.html)"),
        "{md}"
    );
    assert!(md.contains("![pic](a.png)"), "{md}");
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
