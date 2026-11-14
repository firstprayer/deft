use super::*;

#[test]
fn csv_xlsx_json_round_trip() {
    let t = read_csv(b"name,age,id\nAda,36,007\nLin,,12.5\n", "people").unwrap();
    assert_eq!(t.rows[1][1], Cell::Number(36.0));
    assert_eq!(t.rows[1][2], Cell::Text("007".into()));
    assert_eq!(t.rows[2][1], Cell::Empty);
    let x = write_xlsx(std::slice::from_ref(&t)).unwrap();
    assert_eq!(
        write_xlsx(std::slice::from_ref(&t)).unwrap(),
        x,
        "reproducible"
    );
    let back = read_workbook(&x).unwrap();
    assert_eq!(back[0].name, "people");
    assert_eq!(back[0].rows[1][0], Cell::Text("Ada".into()));
    let j = String::from_utf8(write_json(&t).unwrap()).unwrap();
    assert!(j.contains("\"age\": 36"));
    let t2 = read_json(j.as_bytes(), "x").unwrap();
    assert_eq!(t2.rows.len(), 3);
}

#[test]
fn delimiter_and_ranges() {
    let t = read_csv(b"a;b;c\n1;2;3\n", "x").unwrap();
    assert_eq!(t.columns(), 3);
    assert_eq!(parse_range("B2:D10"), Some((1, 1, 9, 3)));
    assert_eq!(parse_range("AA1"), Some((0, 26, 0, 26)));
    assert_eq!(parse_range("nope"), None);
    assert_eq!(
        slice(&t, parse_range("A1:ZZ2").unwrap()).columns(),
        3,
        "ranges stop at the data"
    );
    let s = slice(&t, parse_range("B1:C2").unwrap());
    assert_eq!(s.rows[1], vec![Cell::Number(2.0), Cell::Number(3.0)]);
}

#[test]
fn dates_and_concat() {
    assert_eq!(serial_to_iso(45658.0), "2025-01-01");
    assert_eq!(serial_to_iso(45658.5), "2025-01-01 12:00:00");
    let a = read_csv(b"x,y\n1,2\n", "a").unwrap();
    let b = read_csv(b"x,y\n3,4\n", "b").unwrap();
    assert_eq!(concat(&[a, b]).0.rows.len(), 3);
}

#[test]
fn markdown_table() {
    let t = read_csv(b"a,b\n1,x|y\n", "t").unwrap();
    let md = String::from_utf8(write_markdown(&t)).unwrap();
    assert_eq!(md, "| a | b |\n| --- | --- |\n| 1 | x\\|y |\n");
}

fn sample() -> Table {
    read_csv(
        b"name,city,sales\nAda,Oslo,10\nLin,Rome,5.50\nSam,Oslo,7\nBo,,1\n",
        "s",
    )
    .unwrap()
}

#[test]
fn decimals_keep_their_spelling() {
    let t = read_csv(b"x\n1.50\n1e3\n7\n", "t").unwrap();
    assert_eq!(
        String::from_utf8(write_csv(&t).unwrap()).unwrap(),
        "x\n1.50\n1e3\n7\n"
    );
    assert_eq!(t.rows[1][0].number(), Some(1.5));
}

#[test]
fn filter_grammar() {
    let t = sample();
    let run = |q: &str| {
        let f = parse_filter(q, &t).unwrap();
        filter(&t, &f).rows.len() - 1
    };
    assert_eq!(run("city = 'Oslo'"), 2);
    assert_eq!(run("sales > 5 and city != Rome"), 2);
    assert_eq!(run("sales >= 10 or name ~ '^L'"), 2);
    assert_eq!(run("not (city = oslo)"), 2);
    assert_eq!(run("city is empty"), 1);
    assert_eq!(run("city is not empty"), 3);
    assert_eq!(run("name in (Ada, 'Bo')"), 2);
    assert_eq!(run("name not in (Ada, 'Bo')"), 2);
    assert_eq!(run("name contains 'a'"), 2);
    assert_eq!(run("name startswith a"), 1);
    assert_eq!(run("name endswith 'o'"), 1);
    assert!(
        parse_filter("nope = 1", &t)
            .unwrap_err()
            .contains("columns: name, city, sales")
    );
    assert!(parse_filter("sales >", &t).is_err());
    assert!(parse_filter("(sales > 1", &t).is_err());
}

#[test]
fn sort_is_numeric_and_keeps_empty_last() {
    let t = sample();
    let s = sort(&t, &[2], true);
    let names: Vec<String> = s.rows.iter().skip(1).map(|r| r[0].to_string()).collect();
    assert_eq!(names, ["Ada", "Sam", "Lin", "Bo"]);
    let s = sort(&t, &[1], true);
    assert_eq!(s.rows[4][0].to_string(), "Bo");
}

#[test]
fn dedupe_first_and_last() {
    let t = read_csv(b"k,v\na,1\nb,2\na,3\n", "t").unwrap();
    let first = dedupe(&t, Some(&[0]), false);
    assert_eq!(first.rows[1][1].to_string(), "1");
    assert_eq!(first.rows.len(), 3);
    let last = dedupe(&t, Some(&[0]), true);
    assert_eq!(last.rows[2][1].to_string(), "3");
}

#[test]
fn joins() {
    let l = read_csv(b"id,a\n1,x\n2,y\n", "l").unwrap();
    let r = read_csv(b"id,a,b\n2,p,q\n3,r,s\n", "r").unwrap();
    let text = |t: Table| String::from_utf8(write_csv(&t).unwrap()).unwrap();
    assert_eq!(
        text(join(&l, &r, 0, 0, Join::Inner)),
        "id,a,a_2,b\n2,y,p,q\n"
    );
    assert_eq!(
        text(join(&l, &r, 0, 0, Join::Left)),
        "id,a,a_2,b\n1,x,,\n2,y,p,q\n"
    );
    assert_eq!(
        text(join(&l, &r, 0, 0, Join::Right)),
        "id,a,a_2,b\n2,y,p,q\n3,,r,s\n"
    );
    assert_eq!(
        text(join(&l, &r, 0, 0, Join::Outer)),
        "id,a,a_2,b\n1,x,,\n2,y,p,q\n3,,r,s\n"
    );
}

#[test]
fn pivot_and_group() {
    let t = read_csv(
        b"city,year,n\nOslo,2021,1\nOslo,2022,2\nRome,2021,3\nOslo,2021,4\n",
        "t",
    )
    .unwrap();
    let text = |t: Table| String::from_utf8(write_csv(&t).unwrap()).unwrap();
    assert_eq!(
        text(pivot(&t, 0, 1, Some(2), Agg::Sum)),
        "city,2021,2022\nOslo,5,2\nRome,3,\n"
    );
    assert_eq!(
        text(pivot(&t, 0, 1, None, Agg::Sum)),
        "city,2021,2022\nOslo,2,1\nRome,1,\n"
    );
    assert_eq!(
        text(group(&t, &[0], &[2], Agg::Mean)),
        "city,n_mean\nOslo,2.3333333333\nRome,3\n"
    );
    assert_eq!(
        text(group(&t, &[0], &[], Agg::Sum)),
        "city,count\nOslo,3\nRome,1\n"
    );
}

#[test]
fn stats_and_splits() {
    let t = sample();
    let s = stats(&t);
    assert_eq!(s[1].empty, 1);
    assert_eq!(s[1].unique, 2);
    assert_eq!(s[1].sum, None);
    assert_eq!(s[2].max, Some(10.0));
    let parts = split_by(&t, 1);
    assert_eq!(parts[0].0, "Oslo");
    assert_eq!(parts[0].1.rows.len(), 3);
    assert_eq!(parts[2].0, "empty");
    assert_eq!(split_rows(&t, 3).len(), 2);
    assert_eq!(transpose(&t).rows.len(), 3);
}

#[test]
fn identifiers_are_not_numbers() {
    for s in ["007", "+1234", "-007", "12E3", "12345678901234567890"] {
        assert_eq!(parse_scalar(s, false), Cell::Text(s.into()), "{s}");
    }
    assert_eq!(parse_scalar("1.5e-7", false).number(), Some(1.5e-7));
    assert_eq!(parse_scalar("1E+5", false).number(), Some(1e5));
    // Exactly what a double holds, so a large amount stays a number.
    assert_eq!(
        parse_scalar("15832800000000000", false).number(),
        Some(15832800000000000.0)
    );
}

#[test]
fn comma_decimals_only_in_semicolon_files() {
    assert_eq!(
        parse_scalar("1.234,56", true),
        Cell::Decimal(1234.56, "1.234,56".into())
    );
    assert_eq!(parse_scalar("3,50", false), Cell::Text("3,50".into()));
    assert_eq!(parse_scalar("007,5", true), Cell::Text("007,5".into()));
}

#[test]
fn delimiters_ignore_quotes_and_ties_go_to_the_comma() {
    assert_eq!(detect_delimiter(b"\"a;b;c\",d\n"), b',');
    assert_eq!(detect_delimiter(b"a|b,c\n"), b',');
    assert_eq!(detect_delimiter(b"code\nA|B\n"), b',');
    assert_eq!(detect_delimiter(b"a;b;c\n1,2;3;4\n"), b';');
}

#[test]
fn numeric_reads_thousands_but_not_comma_decimals() {
    assert_eq!(numeric(&Cell::Text("1,200.50".into())), Some(1200.5));
    assert_eq!(numeric(&Cell::Text("3,50".into())), None);
    assert_eq!(numeric(&Cell::Text("12,34,567".into())), None);
    assert_eq!(numeric(&Cell::Text("nan".into())), None);
}

#[test]
fn a_slice_counts_from_a1() {
    let t = Table {
        rows: vec![vec![Cell::Text("x".into())]],
        origin: (2, 2),
        ..Table::default()
    };
    assert_eq!(slice(&t, (2, 2, 2, 2)).rows, t.rows);
    assert_eq!(slice(&t, (0, 0, 1, 1)).rows, vec![vec![Cell::Empty; 2]; 2]);
}

#[test]
fn concat_aligns_by_header() {
    let a = read_csv(b"id,name\n1,Ada\n", "a").unwrap();
    let b = read_csv(b"name,id,age\nLin,2,30\n", "b").unwrap();
    let (t, unrelated) = concat(&[a, b]);
    assert!(!unrelated);
    assert_eq!(write_csv(&t).unwrap(), b"id,name,age\n1,Ada,\n2,Lin,30\n");
}

#[test]
fn json_keeps_long_whole_numbers_exact() {
    let t = read_json(br#"[{"id":12345678901234567890,"v":1.5,"n":7}]"#, "t").unwrap();
    assert_eq!(t.rows[1][0], Cell::Text("12345678901234567890".into()));
    assert_eq!(t.rows[1][1], Cell::Number(1.5));
    assert_eq!(t.rows[1][2], Cell::Number(7.0));
}

#[test]
fn json_keeps_both_of_two_columns_with_one_name() {
    let t = read_csv(b"a,a\n1,2\n", "t").unwrap();
    let out: serde_json::Value = serde_json::from_slice(&write_json(&t).unwrap()).unwrap();
    assert_eq!(out, serde_json::json!([{"a": 1, "a_2": 2}]));
}

#[test]
fn records_inside_a_json_object_become_tables() {
    let vote = br##"{"vote": {"meta": {"date": "2026-11-07"},
        "totals": [{"party": "D", "yea": 1}],
        "records": [{"who": {"@party": "D", "#text": "Adams"}, "vote": "Yea"},
                    {"who": {"@party": "R", "#text": "Baker"}, "vote": "Nay"}]}}"##;
    let tables = read(vote, Format::Json, "vote").unwrap();
    assert_eq!(tables[0].name, "vote.records");
    assert_eq!(tables[1].name, "vote.totals");
    let header: Vec<String> = tables[0].rows[0].iter().map(|c| c.to_string()).collect();
    assert_eq!(header, ["who.@party", "who.#text", "vote"]);
    assert_eq!(tables[0].rows.len(), 3);
    let none = read(br#"{"name": "koa", "version": "2"}"#, Format::Json, "p").unwrap_err();
    assert_eq!(none.code, ErrorCode::InputUnsupported);
}
