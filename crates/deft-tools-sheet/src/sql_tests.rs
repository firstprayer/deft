use super::*;
use crate::read_csv;

fn database(csv: &str) -> Database {
    let mut db = Database::in_memory().unwrap();
    db.add(&read_csv(csv.as_bytes(), "t").unwrap(), "t")
        .unwrap();
    db
}

fn text(t: &Table) -> Vec<Vec<String>> {
    let cells = |r: &Vec<Cell>| r.iter().map(|c| c.to_string()).collect();
    t.rows.iter().map(cells).collect()
}

#[test]
fn source_numbers_come_back_as_written() {
    let db = database("v\n1.50\n537777811.1111111\n0.30000000000000004\n");
    let t = db.query("select v from t", None).unwrap();
    assert_eq!(
        text(&t)[1..],
        [["1.50"], ["537777811.1111111"], ["0.30000000000000004"]]
    );
}

#[test]
fn a_whole_number_keeps_its_decimal_point_but_a_count_has_none() {
    let db = database("v\n2500.0\n2\n");
    let t = db.query("select v from t", None).unwrap();
    assert_eq!(text(&t)[1..], [["2500.0"], ["2"]]);
    let t = db.query("select count(*) n from t", None).unwrap();
    assert_eq!(text(&t)[1], ["2"]);
}

#[test]
fn two_spellings_of_one_value_give_the_plain_one() {
    let db = database("v\n1.50\n1.5\n");
    let t = db.query("select v from t", None).unwrap();
    assert_eq!(text(&t)[1..], [["1.5"], ["1.5"]]);
}

#[test]
fn computed_values_keep_15_digits() {
    let db = database("v\n0.1\n0.2\n");
    let t = db.query("select sum(v) s from t", None).unwrap();
    assert_eq!(text(&t)[1], ["0.3"]);
}

#[test]
fn identifiers_stay_text_and_are_reported_when_mixed_with_numbers() {
    let mut db = Database::in_memory().unwrap();
    let t = read_csv(b"zip\n02134\n10001\n", "t").unwrap();
    assert_eq!(db.add(&t, "t").unwrap(), ["zip"]);
    let out = db.query("select zip from t order by zip", None).unwrap();
    assert_eq!(text(&out)[1..], [["02134"], ["10001"]]);
}

#[test]
fn a_word_next_to_numbers_does_not_turn_them_into_text() {
    let db = database("v\n9\n10\nn/a\n");
    let t = db.query("select v from t where v < 100", None).unwrap();
    assert_eq!(text(&t)[1..], [["9"], ["10"]]);
}

#[test]
fn whole_numbers_beyond_a_double_stay_exact() {
    let db = database("v\n9007199254740993\n1\n");
    let t = db.query("select max(v) m, min(v) n from t", None).unwrap();
    assert_eq!(text(&t)[1], ["9007199254740993", "1"]);
}

#[test]
fn thousands_separators_are_numbers() {
    let db = database("v\n\"1,200\"\n300\n");
    let t = db.query("select sum(v) s from t", None).unwrap();
    assert_eq!(text(&t)[1], ["1500"]);
}

#[test]
fn a_row_wider_than_the_header_keeps_its_cells() {
    let db = database("a\n1,2\n");
    let t = db.query("select column2 from t", None).unwrap();
    assert_eq!(text(&t)[1], ["2"]);
}

#[test]
fn no_rows_still_has_a_header() {
    let db = database("a,b\n1,2\n");
    let t = db.query("select a, b from t where a > 5", None).unwrap();
    assert_eq!(text(&t), [["a", "b"]]);
}

#[test]
fn nothing_can_write() {
    let db = database("a\n1\n");
    for sql in [
        "delete from t",
        "drop table t",
        "create table u(a)",
        "attach 'other.db' as o",
        "vacuum into 'other.db'",
        "select 1; delete from t",
    ] {
        assert!(db.query(sql, None).is_err(), "{sql}");
    }
    assert!(!std::path::Path::new("other.db").exists());
    assert_eq!(text(&db.query("select a from t", None).unwrap())[1], ["1"]);
}

#[test]
fn a_wal_file_gets_no_side_files_and_is_not_changed() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a b.db");
    let conn = Connection::open(&path).unwrap();
    conn.execute_batch("pragma journal_mode = wal; create table t(a); insert into t values (1);")
        .unwrap();
    drop(conn);
    let before = std::fs::read(&path).unwrap();
    let db = Database::open_read_only(&path).unwrap();
    assert_eq!(text(&db.query("select a from t", None).unwrap())[1], ["1"]);
    assert!(db.query("delete from t", None).is_err());
    drop(db);
    assert_eq!(std::fs::read(&path).unwrap(), before);
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
}

#[test]
fn a_wal_file_with_pending_rows_shows_them() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("live.db");
    let writer = Connection::open(&path).unwrap();
    writer
        .execute_batch(
            "pragma journal_mode = wal; pragma wal_autocheckpoint = 0; \
             create table t(a); insert into t values (1);",
        )
        .unwrap();
    let db = Database::open_read_only(&path).unwrap();
    assert_eq!(text(&db.query("select a from t", None).unwrap())[1], ["1"]);
}

#[test]
fn a_damaged_file_is_reported_as_damaged() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("bad.db");
    std::fs::write(&path, b"SQLite format 3\0garbage").unwrap();
    let e = Database::open_read_only(&path)
        .and_then(|db| db.query("select * from sqlite_master", None))
        .err()
        .unwrap();
    assert_eq!(e.code, ErrorCode::InputCorrupt);
}

#[test]
fn a_query_stops_at_the_timeout() {
    let db = database("a\n1\n");
    let forever = "with recursive c(n) as (select 1 union all select n + 1 from c) \
                   select count(*) from c";
    let e = db
        .query(forever, Some(Duration::from_millis(50)))
        .unwrap_err();
    assert_eq!(e.code, ErrorCode::Timeout);
}

#[test]
fn regexp_matches_text_and_numbers_and_skips_empty_cells() {
    let db = database("n,v\nAda,10\nLin,\n");
    let t = db
        .query("select n from t where n regexp '^A'", None)
        .unwrap();
    assert_eq!(text(&t)[1..], [["Ada"]]);
    let t = db
        .query("select n from t where v regexp '0$'", None)
        .unwrap();
    assert_eq!(text(&t)[1..], [["Ada"]]);
    assert!(
        db.query("select n from t where n regexp '('", None)
            .is_err()
    );
}

#[test]
fn a_missing_function_does_not_list_the_tables() {
    let db = database("a\n1\n");
    assert_eq!(
        db.query("select nope(a) from t", None).unwrap_err().hint,
        None
    );
}

#[test]
fn a_missing_table_lists_the_tables() {
    let db = database("a\n1\n");
    let e = db.query("select * from nope", None).unwrap_err();
    assert_eq!(e.hint.as_deref(), Some("tables: t (a)"));
}
