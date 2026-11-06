use super::*;

fn line(y: f64, cells: &[(f64, &str)]) -> Vec<Run> {
    cells
        .iter()
        .map(|(x, text)| Run {
            x: *x,
            y,
            size: 10.0,
            text: text.to_string(),
            width: text.chars().count() as f64 * 5.0,
        })
        .collect()
}

fn two_tables() -> Vec<Vec<Run>> {
    vec![
        line(700.0, &[(50.0, "Sales")]),
        line(680.0, &[(50.0, "Region"), (200.0, "Q1")]),
        line(666.0, &[(50.0, "North"), (200.0, "120")]),
        line(652.0, &[(50.0, "South"), (200.0, "95")]),
        line(
            620.0,
            &[(50.0, "Table 2: costs are listed below this line.")],
        ),
        line(590.0, &[(80.0, "Item"), (160.0, "Unit"), (300.0, "Price")]),
        line(576.0, &[(80.0, "Pen"), (160.0, "Box"), (300.0, "4")]),
        line(562.0, &[(80.0, "Ink"), (160.0, "Pack"), (300.0, "9")]),
    ]
}

#[test]
fn tables_on_a_page_stay_apart_and_text_stays_out() {
    let found = page_tables(&two_tables());
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].rows[0], ["Region", "Q1"]);
    assert_eq!(found[0].rows.len(), 3);
    assert_eq!(found[1].rows[1], ["Pen", "Box", "4"]);
    assert!(!found[0].at_end);
    assert!(found[1].at_end);
}

#[test]
fn a_tall_gap_splits_tables_with_no_text_between() {
    let lines = vec![
        line(700.0, &[(50.0, "a"), (200.0, "b")]),
        line(686.0, &[(50.0, "1"), (200.0, "2")]),
        line(672.0, &[(50.0, "3"), (200.0, "4")]),
        line(600.0, &[(50.0, "c"), (200.0, "d")]),
        line(586.0, &[(50.0, "5"), (200.0, "6")]),
        line(572.0, &[(50.0, "7"), (200.0, "8")]),
    ];
    assert_eq!(page_tables(&lines).len(), 2);
}

#[test]
fn a_table_carries_over_the_page_and_drops_its_repeated_header() {
    let first = vec![
        line(114.0, &[(50.0, "Name"), (200.0, "Age")]),
        line(100.0, &[(50.0, "Ada"), (200.0, "36")]),
        line(86.0, &[(50.0, "Sam"), (200.0, "41")]),
    ];
    let second = vec![
        line(700.0, &[(50.0, "Name"), (200.0, "Age")]),
        line(686.0, &[(50.0, "Lin"), (200.0, "29")]),
        line(672.0, &[(50.0, "Ivo"), (200.0, "52")]),
    ];
    let joined = join_pages(vec![page_tables(&first), page_tables(&second)]);
    assert_eq!(joined.len(), 1);
    assert_eq!(joined[0].len(), 5);
    assert_eq!(joined[0][3], ["Lin", "29"]);
}

#[test]
fn two_columns_of_prose_are_not_a_table() {
    let long = "a line of ordinary prose that runs on for quite a while";
    let lines = vec![
        line(700.0, &[(50.0, long), (320.0, long)]),
        line(686.0, &[(50.0, long), (320.0, long)]),
        line(672.0, &[(50.0, long), (320.0, long)]),
    ];
    assert!(page_tables(&lines).is_empty());
}

#[test]
fn pieces_of_one_phrase_stay_one_cell() {
    let joined = cells(&line(
        700.0,
        &[(50.0, "quick"), (75.0, "brown"), (300.0, "fox")],
    ));
    assert_eq!(joined.len(), 2);
    assert_eq!(joined[0].text, "quick brown");
}

#[test]
fn a_kern_wider_than_a_cell_gap_splits_one_operator() {
    let row = Object::Array(vec![
        Object::string_literal("6.0"),
        Object::Integer(-1500),
        Object::string_literal("73.1"),
    ]);
    let (found, _) = pieces(None, None, &row);
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].1, "6.0");
    assert_eq!(found[1].1, "73.1");
    assert!((found[1].0 - 3.0).abs() < 0.01, "{}", found[1].0);
}

#[test]
fn centred_columns_and_a_group_label_stay_in_the_table() {
    let lines = vec![
        line(700.0, &[(50.0, "Model"), (200.0, "Score")]),
        line(686.0, &[(50.0, "Baselines")]),
        line(672.0, &[(50.0, "Small"), (205.0, "1")]),
        line(658.0, &[(50.0, "Medium"), (198.0, "12.5")]),
    ];
    let found = page_tables(&lines);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].rows.len(), 4);
    assert_eq!(found[0].rows[3], ["Medium", "12.5"]);
}

#[test]
fn a_numbered_equation_is_not_a_table() {
    let lines = vec![
        line(700.0, &[(50.0, "- 5"), (200.0, "- 0.13")]),
        line(686.0, &[(50.0, "L = 2.2"), (300.0, "(13)")]),
        line(672.0, &[(50.0, "x"), (200.0, "y")]),
    ];
    assert!(page_tables(&lines).is_empty());
}

#[test]
fn a_long_centred_label_stays_inside_the_table() {
    let mut text = line(
        800.0,
        &[(50.0, "A paragraph above, to set the left margin.")],
    );
    text[0].width = 500.0;
    let lines = vec![
        text,
        line(
            700.0,
            &[(50.0, "Method"), (200.0, "Score"), (300.0, "Size")],
        ),
        line(
            686.0,
            &[(120.0, "Baselines and their variants, as reported before")],
        ),
        line(672.0, &[(50.0, "Small"), (200.0, "1"), (300.0, "2")]),
        line(658.0, &[(50.0, "Large"), (200.0, "3"), (300.0, "4")]),
    ];
    let found = page_tables(&lines);
    assert_eq!(found[0].rows[0], ["Method", "Score", "Size"]);
    assert_eq!(found[0].rows.len(), 4);
}

#[test]
fn a_tight_header_in_one_piece_is_spread_over_its_columns() {
    let lines = vec![
        line(700.0, &[(50.0, "Range"), (200.0, "0.1 0.2 0.3")]),
        line(
            686.0,
            &[
                (50.0, "Average"),
                (200.0, "10"),
                (225.0, "20"),
                (250.0, "30"),
            ],
        ),
        line(
            672.0,
            &[
                (50.0, "Model a"),
                (200.0, "11"),
                (225.0, "21"),
                (250.0, "31"),
            ],
        ),
        line(
            658.0,
            &[
                (50.0, "Model b"),
                (200.0, "12"),
                (225.0, "22"),
                (250.0, "32"),
            ],
        ),
    ];
    let found = page_tables(&lines);
    assert_eq!(found[0].rows[0], ["Range", "0.1", "0.2", "0.3"]);
}

#[test]
fn authors_are_not_a_table() {
    let lines = vec![
        line(700.0, &[(50.0, "Ada Lovelace"), (250.0, "Lin Wei")]),
        line(
            686.0,
            &[(50.0, "Oslo University"), (250.0, "Bergen University")],
        ),
        line(672.0, &[(50.0, "ada@x.edu"), (250.0, "lin@y.edu")]),
    ];
    assert!(page_tables(&lines).is_empty());
}

fn prose_above_table(above: &str, x: f64) -> Vec<Vec<String>> {
    let lines = vec![
        line(700.0, &[(x, above)]),
        line(680.0, &[(80.0, "Item"), (140.0, "Unit"), (200.0, "Price")]),
        line(666.0, &[(80.0, "Pen"), (140.0, "Box"), (200.0, "4")]),
        line(652.0, &[(80.0, "Ink"), (140.0, "Pack"), (200.0, "9")]),
    ];
    page_tables(&lines).remove(0).rows
}

#[test]
fn a_sentence_above_a_table_stays_out_of_it() {
    // It lies over two of the three columns, with as many words.
    assert_eq!(prose_above_table("Costs follow here.", 70.0)[0][0], "Item");
    assert_eq!(prose_above_table("Costs follow", 70.0)[0][0], "Item");
    assert_eq!(prose_above_table("The costs:", 70.0)[0][0], "Item");
}

#[test]
fn an_abbreviated_header_is_not_a_sentence() {
    let line = line(700.0, &[(80.0, "Model Params. Acc.")]);
    assert!(!ends_sentence(&line));
}
