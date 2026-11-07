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

#[test]
fn preformatted_text_stays_code() {
    let page = "<p>Intro</p><pre class=\"screen\">&lt;p&gt;She had &lt;i&gt;flair&lt;/i&gt;.&lt;/p&gt;\n\n  two</pre>";
    let md = html::to_markdown(page).unwrap();
    assert!(
        md.contains("```\n<p>She had <i>flair</i>.</p>\n\n  two\n```"),
        "{md}"
    );
    let text = html::to_text(page).unwrap();
    assert!(text.contains("<p>She had <i>flair</i>.</p>"), "{text}");
    let fenced =
        html::to_markdown("<pre><code class=\"language-rust\">a ``` b</code></pre>").unwrap();
    assert!(fenced.starts_with("````rust\na ``` b\n````"), "{fenced}");
}

#[test]
fn line_breaks_stay_breaks() {
    let md = html::to_markdown("<p>Section MO<br/>Theorem A<br/>Theorem B</p>").unwrap();
    assert_eq!(md, "Section MO  \nTheorem A  \nTheorem B\n");
    let text = html::to_text("<p>one<br>two</p>").unwrap();
    assert_eq!(text, "one\ntwo\n");
}

#[test]
fn a_bang_before_a_link_is_not_a_picture() {
    let md = html::to_markdown("<p><em>coupole</em>!<a href=\"#note-26\">*</a></p>").unwrap();
    assert_eq!(md, "*coupole*\\![\\*](#note-26)\n");
    let blocks = markdown::parse(&md);
    let Block::Paragraph(spans) = &blocks[0] else {
        panic!("{blocks:?}")
    };
    assert!(spans.iter().all(|s| s.image.is_none()), "{spans:?}");
    assert!(spans.iter().any(|s| s.text == "!"), "{spans:?}");
}

#[test]
fn a_link_around_blocks_goes_on_its_heading() {
    let page = "<ul><li><a href=\"/voting\"><div><h2>Voting</h2></div><div><p>Find out how.</p></div></a></li></ul>";
    let md = html::to_markdown(page).unwrap();
    assert!(md.contains("## [Voting](/voting)"), "{md}");
    assert!(md.contains("Find out how."), "{md}");
    assert!(!md.contains("[##"), "{md}");
}

/// A one-slide deck from raw parts, for structures `pptx::write` never makes.
fn deck(slide: &str, slide_rels: &str, parts: &[(&str, &str)]) -> Vec<u8> {
    use std::io::Write;
    const P: &str = "xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\"";
    const RELS: &str = "xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"";
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    let mut all = vec![
        (
            "ppt/presentation.xml".to_string(),
            format!(
                "<p:presentation {P}><p:sldIdLst><p:sldId id=\"256\" r:id=\"rId1\"/></p:sldIdLst></p:presentation>"
            ),
        ),
        (
            "ppt/_rels/presentation.xml.rels".to_string(),
            format!(
                "<Relationships {RELS}><Relationship Id=\"rId1\" Type=\"slide\" Target=\"slides/slide1.xml\"/></Relationships>"
            ),
        ),
        (
            "ppt/slides/slide1.xml".to_string(),
            format!("<p:sld {P}><p:cSld><p:spTree>{slide}</p:spTree></p:cSld></p:sld>"),
        ),
        (
            "ppt/slides/_rels/slide1.xml.rels".to_string(),
            format!("<Relationships {RELS}>{slide_rels}</Relationships>"),
        ),
    ];
    all.extend(parts.iter().map(|(n, x)| (n.to_string(), x.to_string())));
    for (name, xml) in all {
        zip.start_file(name, options).unwrap();
        zip.write_all(xml.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn smartart_text_is_read_in_diagram_order() {
    let frame = "<p:graphicFrame><a:graphic><a:graphicData><dgm:relIds xmlns:dgm=\"http://schemas.openxmlformats.org/drawingml/2006/diagram\" r:dm=\"rId2\"/></a:graphicData></a:graphic></p:graphicFrame>";
    let rels = "<Relationship Id=\"rId2\" Type=\"diagramData\" Target=\"../diagrams/data1.xml\"/>";
    let pt = |id: &str, text: &str| {
        format!(
            "<dgm:pt modelId=\"{id}\"><dgm:t><a:p><a:r><a:t>{text}</a:t></a:r></a:p></dgm:t></dgm:pt>"
        )
    };
    // The file lists the second postulate first; the connections say otherwise.
    let data = format!(
        "<dgm:dataModel xmlns:dgm=\"http://schemas.openxmlformats.org/drawingml/2006/diagram\" xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\"><dgm:ptLst><dgm:pt modelId=\"0\" type=\"doc\"/>{}{}<dgm:pt modelId=\"9\" type=\"sibTrans\"><dgm:t><a:p><a:r><a:t>not text</a:t></a:r></a:p></dgm:t></dgm:pt></dgm:ptLst><dgm:cxnLst><dgm:cxn modelId=\"c1\" srcId=\"0\" destId=\"1\" srcOrd=\"0\"/><dgm:cxn modelId=\"c2\" srcId=\"0\" destId=\"2\" srcOrd=\"1\"/><dgm:cxn modelId=\"c3\" type=\"presOf\" srcId=\"2\" destId=\"0\" srcOrd=\"0\"/></dgm:cxnLst></dgm:dataModel>",
        pt("2", "2- A line can be extended."),
        pt("1", "1- A line can be drawn."),
    );
    let bytes = deck(frame, rels, &[("ppt/diagrams/data1.xml", &data)]);
    let (slides, _) = pptx::read(&bytes).unwrap();
    let text: Vec<String> = slides[0]
        .body
        .iter()
        .map(|p| p.iter().map(|s| s.text.as_str()).collect())
        .collect();
    assert_eq!(
        text,
        ["1- A line can be drawn.", "2- A line can be extended."]
    );
}

#[test]
fn slide_tables_links_and_untitled_slides() {
    let title = "<p:sp><p:nvSpPr><p:nvPr><p:ph type=\"title\"/></p:nvPr></p:nvSpPr><p:txBody><a:p><a:r><a:t> </a:t></a:r></a:p></p:txBody></p:sp>";
    let link = "<p:sp><p:txBody><a:p><a:r><a:rPr><a:hlinkClick r:id=\"rId4\"/></a:rPr><a:t>This is </a:t></a:r><a:r><a:rPr><a:hlinkClick r:id=\"rId4\"/></a:rPr><a:t>a link</a:t></a:r></a:p></p:txBody></p:sp>";
    let cell = |a: &str, b: &str| {
        format!(
            "<a:tc><a:txBody><a:p><a:r><a:t>{a}</a:t></a:r></a:p><a:p><a:r><a:t>{b}</a:t></a:r></a:p></a:txBody></a:tc>"
        )
    };
    let table = format!(
        "<p:graphicFrame><a:graphic><a:graphicData><a:tbl><a:tr>{}{}</a:tr><a:tr>{}{}</a:tr></a:tbl></a:graphicData></a:graphic></p:graphicFrame>",
        cell("How", ""),
        cell("Result", ""),
        cell("Sessions.", "Communities."),
        cell("Exchanges.", "Cohesion.")
    );
    let rels = "<Relationship Id=\"rId4\" Type=\"hyperlink\" Target=\"http://tika.apache.org/\" TargetMode=\"External\"/>";
    let bytes = deck(&format!("{title}{link}{table}"), rels, &[]);
    let (slides, images) = pptx::read(&bytes).unwrap();
    let md = markdown::render(&pptx::to_document(&slides, images, false));
    assert!(md.starts_with("## Slide 1\n"), "{md}");
    assert!(
        md.contains("- [This is a link](http://tika.apache.org/)"),
        "{md}"
    );
    assert!(
        md.contains(
            "| How | Result |\n| --- | --- |\n| Sessions. Communities. | Exchanges. Cohesion. |"
        ),
        "{md}"
    );
}

#[test]
fn book_chapters_become_valid_xhtml() {
    let one = "<h1 id=\"a\">A</h1><p align=\"center\">x&nbsp;y <Listing number=\"1\"><pre>code</pre></Listing> <tt>t</tt></p><p><a href=\"#b\">to b</a> <a href=\"#nowhere\">dead</a> <a href=\"other.md\">file</a> <img src=\"web.png\" alt=\"pic\"><img src=\"in.png\" alt=\"kept\"><br><input type=\"checkbox\" checked disabled></p><script>x()</script>";
    let two = "<h2 id=\"b\">B</h2><p id=\"b\">again</p>";
    let names = ["c1.xhtml".to_string(), "c2.xhtml".to_string()];
    let has = |src: &str| src == "in.png";
    let out = xhtml::for_book(&[one.to_string(), two.to_string()], &names, &has);
    for body in &out.bodies {
        let doc = format!("<body xmlns=\"http://www.w3.org/1999/xhtml\">{body}</body>");
        roxmltree::Document::parse(&doc).unwrap_or_else(|e| panic!("{e}: {body}"));
    }
    let first = &out.bodies[0];
    assert!(first.contains("x\u{a0}y"), "{first}");
    assert!(
        !first.contains("isting") && !first.contains("align"),
        "{first}"
    );
    assert!(
        first.contains("<pre>code</pre>") && first.contains("<code>t</code>"),
        "{first}"
    );
    assert!(first.contains("<a href=\"c2.xhtml#b\">to b</a>"), "{first}");
    assert!(
        first.contains(" dead ") && first.contains(" file "),
        "{first}"
    );
    assert!(
        first.contains("pic<img src=\"in.png\" alt=\"kept\"/><br/>"),
        "{first}"
    );
    assert!(first.contains("checked=\"checked\""), "{first}");
    assert!(
        !first.contains("script") && !first.contains("x()"),
        "{first}"
    );
    assert_eq!(out.bodies[1].matches("id=\"b\"").count(), 1);
    assert_eq!(out.pictures_left_out, 1);
    assert!(out.markup_dropped >= 3);
}

#[test]
fn word_links_to_headings_use_bookmarks() {
    let doc = Document {
        blocks: markdown::parse(
            "# Intro\n\nSee [the end](#the-end) or [web](https://a.example).\n\n## The End\n\n## The End\n",
        ),
        ..Default::default()
    };
    let bytes = docx::write(&doc).unwrap();
    let pkg = zipped::Package::open(&bytes).unwrap();
    let body = pkg.text("word/document.xml").unwrap();
    let rels = pkg.text("word/_rels/document.xml.rels").unwrap();
    assert!(
        body.contains("<w:hyperlink w:anchor=\"the-end\">"),
        "{body}"
    );
    for name in ["intro", "the-end", "the-end-1"] {
        assert!(body.contains(&format!("w:name=\"{name}\"")), "{body}");
    }
    assert!(!rels.contains("Target=\"#"), "{rels}");
    assert!(rels.contains("https://a.example"), "{rels}");
}

#[test]
fn a_rule_before_a_heading_is_not_front_matter() {
    let md = "# Title\n\n### 1. One\n\ntext\n\n---\n### 2. Two\n\nmore\n\n---\n## Three\n";
    let names: Vec<String> = markdown::headings(md)
        .into_iter()
        .map(|h| h.plain)
        .collect();
    assert_eq!(names, ["Title", "1. One", "2. Two", "Three"]);
    let fronted = "---\ntitle: x\n---\n# Real\n";
    let found = markdown::headings(fronted);
    assert_eq!(found.len(), 1);
    assert_eq!((found[0].plain.as_str(), found[0].line), ("Real", 3));
}

#[test]
fn rtf_decodes_each_font_code_page() {
    let cp1250 = rtf::read(br"{\rtf1\ansi\ansicpg1250{\fonttbl{\f0\fcharset238 Arial CE;}}\f0 za\'bf\'f3\'b3\'e6 g\'ea\'9cl\'b9\par}");
    assert_eq!(cp1250.text(), "zażółć gęślą\n");
    // Shift-JIS in a \fcharset128 font, with a trail byte written as a letter.
    let japanese = rtf::read(br"{\rtf1\ansi\ansicpg1252{\fonttbl{\f0\fcharset0 Times;}{\f1\fcharset128 MS Gothic;}}\f0 caf\'e9 {\f1 \'83e\'83X\'83g \'94\'4f}\par}");
    assert_eq!(japanese.text(), "café テスト 念\n");
    // \uc2: two bytes stand for each \u character.
    let unicode = rtf::read(
        br"{\rtf1\ansi{\fonttbl{\f1\fcharset128 MS Gothic;}}\f1\uc2 \u24565\'94\'4f!\par}",
    );
    assert_eq!(unicode.text(), "念!\n");
    let meta = rtf::read(
        br"{\rtf1\ansi{\info{\upr{\author Axel D?fler}{\*\ud{\author Axel D\u246?fler}}}}Text\par}",
    );
    assert_eq!(meta.author.as_deref(), Some("Axel Döfler"));
    assert_eq!(meta.text(), "Text\n");
}

#[test]
fn rtf_keeps_tables_lists_notes_links_and_pictures() {
    let png = "89504e470d0a1a0a";
    let src = format!(
        r#"{{\rtf1\ansi{{\fonttbl{{\f0 Arial;}}{{\f3\fcharset2 Symbol;}}}}{{\stylesheet{{\s1 heading 1;}}{{\*\cs10 Default;}}}}
\pard\s1 Title\par
\pard Claim{{\super\chftn}}{{\footnote \pard\plain {{\chftn}} The source.}} here.\par
{{\listtext\pard\plain\f3 \'b7\tab}}\pard\li720 one\par
{{\listtext\pard\plain\f3 \'b7\tab}}\pard\li1440 deeper\par
{{\listtext\pard\plain 2.\tab}}\pard\li720 two\par
\pard See {{\field{{\*\fldinst HYPERLINK "http://tika.apache.org"}}{{\fldrslt tika}}}}.\par
\trowd\cellx1000\cellx2000\pard\intbl a\cell b\cell\row
\pard\intbl c\par d\cell e\cell\row
\pard After {{\*\shppict{{\pict\pngblip {png}}}}}{{\nonshppict{{\pict\wmetafile8 0102}}}}\par}}"#
    );
    let doc = rtf::read(src.as_bytes());
    let md = markdown::render(&doc);
    assert!(md.starts_with("# Title\n"), "{md}");
    assert!(md.contains("Claim[^1] here."), "{md}");
    assert!(md.contains("[^1]: The source."), "{md}");
    assert!(md.contains("- one\n  - deeper\n"), "{md}");
    assert!(md.contains("See [tika](http://tika.apache.org)."), "{md}");
    assert!(md.contains("| a | b |\n| --- | --- |\n| c d | e |"), "{md}");
    assert_eq!(doc.images.len(), 1);
    assert_eq!(doc.images[0].name, "image1.png");
    assert!(md.contains("After ![](image1.png)"), "{md}");
}

fn odt(content: &str, manifest: &str, parts: &[(&str, &[u8])]) -> Vec<u8> {
    use std::io::Write;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    let ns = "xmlns:office=\"urn:oasis:names:tc:opendocument:xmlns:office:1.0\" xmlns:text=\"urn:oasis:names:tc:opendocument:xmlns:text:1.0\" xmlns:draw=\"urn:oasis:names:tc:opendocument:xmlns:drawing:1.0\" xmlns:svg=\"urn:oasis:names:tc:opendocument:xmlns:svg-compatible:1.0\" xmlns:xlink=\"http://www.w3.org/1999/xlink\" xmlns:style=\"urn:oasis:names:tc:opendocument:xmlns:style:1.0\"";
    let xml = format!(
        "<office:document-content {ns}><office:body><office:text>{content}</office:text></office:body></office:document-content>"
    );
    let mut all: Vec<(&str, Vec<u8>)> = vec![
        ("content.xml", xml.into_bytes()),
        ("META-INF/manifest.xml", manifest.as_bytes().to_vec()),
    ];
    all.extend(parts.iter().map(|(n, b)| (*n, b.to_vec())));
    for (name, data) in all {
        zip.start_file(name, options).unwrap();
        zip.write_all(&data).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn odt_keeps_notes_fields_shapes_indexes_and_picture_links() {
    let content = "<text:tracked-changes><text:changed-region><text:deletion><text:p>gone</text:p></text:deletion></text:changed-region></text:tracked-changes>\
<text:h text:outline-level=\"1\">Apache Tika <office:annotation><text:p>a comment</text:p></office:annotation>Test</text:h>\
<text:p>Rather<text:note text:id=\"n1\"><text:note-citation>1</text:note-citation><text:note-body><text:p>A simple footnote</text:p></text:note-body></text:note> difficult.</text:p>\
<text:p><draw:rect><svg:title>Rect</svg:title><text:p>A rectangle</text:p></draw:rect></text:p>\
<text:p>Project <text:bibliography-mark text:identifier=\"Solr\">[Solr]</text:bibliography-mark>, title: <text:bookmark-ref>Apache Tika Test</text:bookmark-ref>, by<text:s text:c=\"2\"/>me</text:p>\
<text:p><draw:a xlink:href=\"https://tika.apache.org/\"><draw:frame><draw:image xlink:href=\"Pictures/p.png\"/><svg:title>Visit Tika</svg:title></draw:frame></draw:a></text:p>\
<text:bibliography><text:bibliography-source><text:index-title-template>Template</text:index-title-template></text:bibliography-source><text:index-body><text:index-title><text:p>Bibliography</text:p></text:index-title><text:p>Solr: Various, 2009</text:p></text:index-body></text:bibliography>";
    let bytes = odt(content, "<manifest/>", &[("Pictures/p.png", b"\x89PNG")]);
    let doc = odf::read(&bytes).unwrap();
    let md = markdown::render(&doc);
    assert!(md.starts_with("# Apache Tika Test\n"), "{md}");
    assert!(md.contains("Rather[^1] difficult."), "{md}");
    assert!(md.contains("[^1]: A simple footnote"), "{md}");
    assert!(md.contains("A rectangle") && !md.contains("Rect\n"), "{md}");
    assert!(
        md.contains("Project \\[Solr\\], title: Apache Tika Test, by  me"),
        "{md}"
    );
    assert!(
        md.contains("[![Visit Tika](p.png)](https://tika.apache.org/)"),
        "{md}"
    );
    assert!(md.contains("Bibliography\n\nSolr: Various, 2009"), "{md}");
    assert!(
        !md.contains("gone") && !md.contains("comment") && !md.contains("Template"),
        "{md}"
    );
    let locked = odt("<text:p>x</text:p>", "<manifest:encryption-data/>", &[]);
    let e = odf::read(&locked).unwrap_err();
    assert_eq!(e.code, deft_core::error::ErrorCode::InputEncrypted);
}

#[test]
fn book_pictures_keep_their_titles_and_spaces() {
    let book = epub::Book {
        chapters: vec!["<p><img src=\"images/cover.png\" alt=\"Cover Image\" title=\"A/B\"/> <img src=\"../img/my%20pic.jpg\" alt=\"\"/></p>".into()],
        ..Default::default()
    };
    let md = book.markdown().unwrap();
    assert!(md.contains("![Cover Image](cover.png \"A/B\")"), "{md}");
    assert!(md.contains("![](my%20pic.jpg)"), "{md}");
    let doc = Document {
        blocks: vec![Block::Paragraph(vec![Span {
            image: Some("Object 1".into()),
            ..Default::default()
        }])],
        ..Default::default()
    };
    assert_eq!(markdown::render(&doc), "![](Object%201)\n");
}
