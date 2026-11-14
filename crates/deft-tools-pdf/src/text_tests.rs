use super::*;

fn glyph(text: &str, x: f64, width: f64, run: usize) -> Glyph {
    Glyph {
        text: text.to_string(),
        start: Point::new(x, 100.0),
        end: Point::new(x + width, 100.0),
        up: Vec2::new(0.0, -10.0),
        run,
    }
}

fn page(glyphs: Vec<Glyph>) -> PageText {
    PageText {
        glyphs,
        width: 612.0,
        height: 792.0,
        images: vec![],
    }
}

#[test]
fn a_gap_wider_than_a_cell_splits_one_operator() {
    let runs = page(vec![glyph("6", 10.0, 5.0, 0), glyph("7", 30.0, 5.0, 0)]).runs();
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].text, "6");
    assert_eq!(runs[1].text, "7");
    assert!((runs[1].x - 30.0).abs() < 0.01, "{}", runs[1].x);
    assert!((runs[1].y - 692.0).abs() < 0.01, "{}", runs[1].y);
}

#[test]
fn a_word_gap_is_a_space_and_kerning_is_not() {
    let text = page(vec![
        glyph("a", 10.0, 5.0, 0),
        glyph("b", 15.5, 5.0, 0),
        glyph("c", 24.0, 5.0, 0),
    ])
    .text();
    assert_eq!(text, "ab c");
}

#[test]
fn a_match_over_a_line_break_gets_a_box_on_each_line() {
    let mut next = glyph("b", 10.0, 5.0, 1);
    next.start.y = 120.0;
    next.end.y = 120.0;
    let hits = page(vec![glyph("a", 10.0, 5.0, 0), next]).find("A B");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].regions.len(), 2);
}

#[test]
fn a_box_covers_the_match_and_not_the_rest_of_the_line() {
    let line = page(vec![
        glyph("a", 10.0, 5.0, 0),
        glyph("b", 15.0, 5.0, 0),
        glyph("c", 20.0, 5.0, 0),
    ]);
    let hits = line.find("b");
    let [x, _, w, _] = hits[0].regions[0];
    assert!((x - 14.0).abs() < 0.01 && (w - 7.0).abs() < 0.01, "{x} {w}");
}

fn wrapped(first: &str, second: &str) -> PageText {
    let mut glyphs: Vec<Glyph> = first
        .chars()
        .enumerate()
        .map(|(i, c)| glyph(&c.to_string(), 10.0 + i as f64 * 5.0, 5.0, 0))
        .collect();
    for (i, c) in second.chars().enumerate() {
        let mut g = glyph(&c.to_string(), 10.0 + i as f64 * 5.0, 5.0, 1);
        g.start.y = 120.0;
        g.end.y = 120.0;
        glyphs.push(g);
    }
    page(glyphs)
}

#[test]
fn a_word_split_at_a_line_end_is_found_on_both_lines() {
    let hits = wrapped("the de-", "duction").find("deduction");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].regions.len(), 2);
}

#[test]
fn a_real_hyphen_at_a_line_end_still_matches() {
    assert_eq!(wrapped("well-", "known").find("well-known").len(), 1);
    assert_eq!(wrapped("well-", "known").find("wellknown").len(), 1);
}

#[test]
fn a_hyphen_inside_a_line_is_not_passed_over() {
    assert!(
        page(vec![
            glyph("a", 10.0, 5.0, 0),
            glyph("-", 15.0, 5.0, 0),
            glyph("b", 20.0, 5.0, 0),
        ])
        .find("ab")
        .is_empty()
    );
}

#[test]
fn ligatures_read_as_letters() {
    assert_eq!(plain("e\u{FB03}cient \u{FB02}ow"), "efficient flow");
}

#[test]
fn undecoded_counts_only_what_is_not_white_space() {
    assert!((undecoded("ab \u{FFFD}\u{FFFD}") - 0.5).abs() < 0.001);
    assert_eq!(undecoded(""), 0.0);
}

#[test]
fn arabic_drawn_left_to_right_reads_right_to_left() {
    // "عربي" as shaped glyphs drawn from the left: the last letter first.
    let shapes = ["\u{FEF2}", "\u{FE92}", "\u{FEAE}", "\u{FECB}"];
    let mut glyphs: Vec<Glyph> = shapes
        .iter()
        .enumerate()
        .map(|(i, s)| glyph(&plain(s), 10.0 + i as f64 * 5.0, 5.0, 0))
        .collect();
    glyphs.push(glyph(" ", 30.0, 3.0, 0));
    glyphs.push(glyph("x", 40.0, 5.0, 0));
    assert_eq!(page(glyphs).text(), "عربي x");
    // A glyph that names a whole word is in reading order already.
    let word = page(vec![glyph("حبيبي", 10.0, 30.0, 0)]);
    assert_eq!(word.text(), "حبيبي");
}

#[test]
fn chinese_has_no_spaces_between_characters() {
    let text = page(vec![
        glyph("关", 10.0, 5.0, 0),
        glyph("于", 18.0, 5.0, 0),
        glyph("x", 40.0, 5.0, 0),
    ])
    .text();
    assert_eq!(text, "关于 x");
}

#[test]
fn gbk_bytes_join_in_pairs() {
    let byte = |b: u32| {
        char::from_u32(crate::fonts::GBK_BYTES + b - 0x80)
            .unwrap()
            .to_string()
    };
    // 目 is C4 BF in GBK, padded with a space as old writers do.
    let mut glyphs = vec![
        glyph(&byte(0xC4), 10.0, 5.0, 0),
        glyph(&byte(0xBF), 15.0, 5.0, 0),
        glyph(" ", 20.0, 3.0, 0),
        glyph(&byte(0xC2), 30.0, 5.0, 0),
        glyph(&byte(0xBC), 35.0, 5.0, 0),
    ];
    join_gbk(&mut glyphs);
    assert_eq!(page(glyphs).text(), "目录");
}

#[test]
fn vertical_text_reads_down_the_column() {
    let down = |text: &str, y: f64| Glyph {
        text: text.to_string(),
        start: Point::new(100.0, y),
        end: Point::new(110.0, y),
        up: Vec2::new(0.0, -10.0),
        run: 0,
    };
    let text = page(vec![
        down("あ", 100.0),
        down("い", 110.0),
        down("う", 120.0),
    ])
    .text();
    assert_eq!(text, "あいう");
}

#[test]
fn a_picture_with_a_short_stamp_of_text_is_a_scan() {
    let stamp: Vec<Glyph> = "Published: October 2, 1880 Copyright The New York Times"
        .chars()
        .enumerate()
        .map(|(i, c)| glyph(&c.to_string(), 10.0 + i as f64 * 5.0, 5.0, 0))
        .collect();
    let mut scan = page(stamp.clone());
    scan.images = vec![[0.0, 0.0, 612.0, 792.0]];
    assert!(scan.chars() >= 20);
    assert!(scan.is_scan());
    // The same line on a page without a picture is a text page.
    assert!(!page(stamp).is_scan());
    // A full text layer over the picture makes it searchable already.
    let mut layer = Vec::new();
    for line in 0..60 {
        for i in 0..90 {
            let mut g = glyph("e", 10.0 + i as f64 * 6.5, 6.0, line);
            g.start.y = 20.0 + line as f64 * 12.0;
            g.end.y = g.start.y;
            layer.push(g);
        }
    }
    let mut ocred = page(layer);
    ocred.images = vec![[0.0, 0.0, 612.0, 792.0]];
    assert!(!ocred.is_scan(), "{}", ocred.text_cover());
}
