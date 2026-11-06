use super::*;

fn line(y: f64, cells: &[(f64, &str)]) -> Vec<Run> {
    cells
        .iter()
        .map(|(x, text)| Run {
            x: *x,
            y,
            size: 10.0,
            text: text.to_string(),
        })
        .collect()
}

fn two_tables() -> Vec<Vec<Run>> {
    vec![
        line(700.0, &[(50.0, "Sales")]),
        line(680.0, &[(50.0, "Region"), (200.0, "Q1")]),
        line(666.0, &[(50.0, "North"), (200.0, "120")]),
        line(652.0, &[(50.0, "South"), (200.0, "95")]),
        line(620.0, &[(50.0, "Costs are below.")]),
        line(590.0, &[(80.0, "Item"), (160.0, "Unit"), (300.0, "Price")]),
        line(576.0, &[(80.0, "Pen"), (160.0, "Box"), (300.0, "4")]),
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
    ];
    assert_eq!(page_tables(&lines).len(), 2);
}

#[test]
fn a_table_carries_over_the_page_and_drops_its_repeated_header() {
    let first = vec![
        line(100.0, &[(50.0, "Name"), (200.0, "Age")]),
        line(86.0, &[(50.0, "Ada"), (200.0, "36")]),
    ];
    let second = vec![
        line(700.0, &[(50.0, "Name"), (200.0, "Age")]),
        line(686.0, &[(50.0, "Lin"), (200.0, "29")]),
    ];
    let joined = join_pages(vec![page_tables(&first), page_tables(&second)]);
    assert_eq!(joined.len(), 1);
    assert_eq!(joined[0].len(), 3);
    assert_eq!(joined[0][2], ["Lin", "29"]);
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
