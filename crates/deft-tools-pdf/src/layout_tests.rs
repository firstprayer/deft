use super::*;

fn sized(y: f64, size: f64, cells: &[(f64, &str)]) -> Vec<Run> {
    let mut l = line(y, cells);
    for r in &mut l {
        r.size = size;
    }
    l
}

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
        &[(50.0, "quick"), (78.0, "brown"), (300.0, "fox")],
    ));
    assert_eq!(joined.len(), 2);
    assert_eq!(joined[0].text, "quick brown");
}

#[test]
fn letters_drawn_one_by_one_make_one_word() {
    // Odoo invoices draw each letter with its own operator.
    let joined = cells(&line(
        700.0,
        &[(50.0, "T"), (55.0, "a"), (60.0, "x"), (68.0, "due")],
    ));
    assert_eq!(joined.len(), 1);
    assert_eq!(joined[0].text, "Tax due");
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

fn item(y: f64, name: &str, price: &str) -> Vec<Run> {
    line(
        y,
        &[(50.0, name), (200.0, "1"), (260.0, price), (330.0, price)],
    )
}

#[test]
fn group_labels_and_notes_stay_inside_the_table() {
    // As an Odoo invoice: groups, a note under an item and a subtotal.
    let lines = vec![
        line(
            700.0,
            &[
                (50.0, "Item"),
                (200.0, "Qty"),
                (260.0, "Price"),
                (330.0, "Total"),
            ],
        ),
        line(686.0, &[(50.0, "Non Food")]),
        item(672.0, "Beeswax", "42.00"),
        line(658.0, &[(60.0, "Acme beeswax")]),
        item(644.0, "Chair", "70.00"),
        line(630.0, &[(330.0, "Subtotal 112.00")]),
        line(616.0, &[(50.0, "Food")]),
        item(602.0, "Olive oil", "0.90"),
        item(588.0, "Truffles", "150.00"),
    ];
    let found = page_tables(&lines);
    assert_eq!(found.len(), 1);
    let names: Vec<&str> = found[0].rows.iter().map(|r| r[0].as_str()).collect();
    assert!(
        names.contains(&"Beeswax") && names.contains(&"Truffles"),
        "{names:?}"
    );
}

#[test]
fn a_header_over_several_lines_is_one_row() {
    let lines = vec![
        line(700.0, &[(200.0, "Retail"), (300.0, "Percent")]),
        line(686.0, &[(200.0, "Sales"), (300.0, "Change")]),
        line(672.0, &[(50.0, "Q2"), (200.0, "1,826"), (300.0, "0.5")]),
        line(658.0, &[(50.0, "Q1"), (200.0, "1,818"), (300.0, "-0.1")]),
        line(644.0, &[(50.0, "Q4"), (200.0, "1,821"), (300.0, "0.5")]),
    ];
    let found = page_tables(&lines);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].rows[0], ["", "Retail Sales", "Percent Change"]);
    assert_eq!(found[0].rows[1][0], "Q2");
}

#[test]
fn lines_a_wide_gap_above_and_a_smaller_footer_below_stay_out() {
    let lines = vec![
        line(760.0, &[(50.0, "Invoice no."), (200.0, "30064443")]),
        line(746.0, &[(50.0, "Date"), (200.0, "7 May")]),
        line(
            700.0,
            &[
                (50.0, "Item"),
                (200.0, "Qty"),
                (260.0, "Price"),
                (330.0, "Total"),
            ],
        ),
        item(686.0, "Exchange", "3.89"),
        item(672.0, "Exchange", "5.39"),
        item(658.0, "Exchange", "5.39"),
        sized(
            640.0,
            7.0,
            &[(50.0, "Company AG"), (200.0, "Board"), (330.0, "Bank")],
        ),
        sized(
            632.0,
            7.0,
            &[(50.0, "Street 40"), (200.0, "Chair"), (330.0, "IBAN")],
        ),
    ];
    let found = page_tables(&lines);
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].rows[0][0], "Item");
    assert_eq!(found[0].rows.last().unwrap()[0], "Exchange");
}

/// A page in two columns: each has a table, and the right one also prose,
/// whose lines fall between the rows of the left one.
fn two_column_page() -> Vec<Run> {
    let mut runs = Vec::new();
    for i in 0..8 {
        let y = 700.0 - i as f64 * 14.0;
        let n = format!("{}", 100 + i);
        runs.extend(line(
            y,
            &[
                (40.0, &n),
                (70.0, "Outlays"),
                (200.0, "1,200"),
                (250.0, "380"),
            ],
        ));
    }
    for i in 0..6 {
        let y = 693.0 - i as f64 * 14.0;
        let n = format!("{}", 900 + i);
        runs.extend(line(
            y,
            &[
                (320.0, &n),
                (350.0, "Farm loans"),
                (480.0, "-0.5"),
                (530.0, "-0.7"),
            ],
        ));
    }
    for i in 0..5 {
        let y = 600.0 - i as f64 * 11.0;
        runs.extend(line(
            y,
            &[(320.0, "The program makes loans to farmers who cannot")],
        ));
    }
    runs
}

#[test]
fn tables_side_by_side_in_two_columns_stay_apart() {
    let found = runs_tables(two_column_page(), 612.0, 792.0);
    assert_eq!(found.len(), 2, "{found:?}");
    assert_eq!(found[0].rows[0], ["100", "Outlays", "1,200", "380"]);
    assert_eq!(found[1].rows[0], ["900", "Farm loans", "-0.5", "-0.7"]);
    assert!(found.iter().all(|t| t.rows.iter().all(|r| r.len() == 4)));
}

#[test]
fn one_table_with_a_wide_gap_is_not_cut_in_two() {
    let mut runs = Vec::new();
    for i in 0..10 {
        let y = 700.0 - i as f64 * 14.0;
        runs.extend(line(
            y,
            &[(40.0, "Item"), (100.0, "12"), (450.0, "Box"), (520.0, "4")],
        ));
    }
    let found = runs_tables(runs, 612.0, 792.0);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].rows[0].len(), 4);
}

#[test]
fn line_numbers_beside_the_page_are_not_a_column() {
    let mut runs = Vec::new();
    for i in 0..12 {
        let y = 700.0 - i as f64 * 14.0;
        runs.extend(line(y, &[(-26.0, &format!("{}", i + 1))]));
        runs.extend(line(y, &[(48.0, "a line of running text in a book")]));
    }
    assert!(runs_tables(runs, 432.0, 648.0).is_empty());
}

fn block(rows: &[&[(f64, &str)]]) -> Vec<Vec<Run>> {
    rows.iter()
        .enumerate()
        .map(|(i, cells)| line(700.0 - i as f64 * 14.0, cells))
        .collect()
}

#[test]
fn contents_lists_indexes_and_formulas_are_not_tables() {
    let contents = block(&[
        &[(50.0, "PREFACE"), (300.0, "xiii")],
        &[(50.0, "INTRODUCTION"), (300.0, "1")],
        &[(50.0, "CHAPTER ONE: Creators"), (300.0, "21")],
        &[(50.0, "CHAPTER TWO: Mere Copyists"), (300.0, "31")],
        &[(50.0, "CHAPTER THREE: Catalogs"), (300.0, "48")],
    ]);
    assert!(page_tables(&contents).is_empty());
    let list = block(&[
        &[(50.0, "1."), (70.0, "name of the service;")],
        &[(50.0, "2."), (70.0, "channel of the program;")],
        &[(50.0, "3."), (70.0, "type of program;")],
        &[(50.0, "4."), (70.0, "date of transmission;")],
    ]);
    assert!(page_tables(&list).is_empty());
    let index = block(&[
        &[
            (50.0, "Barlow, Joel, 8, 112"),
            (250.0, "Bunyan, John, 93, 94"),
        ],
        &[
            (50.0, "Barnes & Noble, 147"),
            (250.0, "Burdick, Quentin, 60"),
        ],
        &[
            (50.0, "Barry, Hank, 189, 191"),
            (250.0, "Bush, George W., 323n"),
        ],
        &[
            (50.0, "Bell, Alexander, 60"),
            (250.0, "Byrd, Robert, 112, 120"),
        ],
    ]);
    assert!(page_tables(&index).is_empty());
    let cases = block(&[
        &[(50.0, "d(x,y) ="), (120.0, "0"), (160.0, "falls x = y")],
        &[(120.0, "1"), (160.0, "falls x ≠ y")],
        &[(50.0, "⇒"), (120.0, "A ∩ B"), (160.0, "∪ C")],
    ]);
    assert!(page_tables(&cases).is_empty());
}
