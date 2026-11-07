use super::*;

#[test]
fn yaml_merge_keys_and_numeric_keys_survive() {
    let v = parse(
        b"base: &b {a: 1}\nuse:\n  <<: *b\n  c: 2\n3: x\n",
        Format::Yaml,
    )
    .unwrap();
    assert_eq!(v["use"]["a"], json!(1));
    assert_eq!(v["3"], json!("x"));
}

#[test]
fn several_yaml_documents_become_a_list() {
    let v = parse(b"a: 1\n---\na: 2\n", Format::Yaml).unwrap();
    assert_eq!(v, json!([{"a": 1}, {"a": 2}]));
}

#[test]
fn toml_keeps_dates_as_text() {
    let v = parse(b"when = 2026-01-02\n[t]\nx = 1\n", Format::Toml).unwrap();
    assert_eq!(v["when"], json!("2026-01-02"));
    assert_eq!(v["t"]["x"], json!(1));
}

#[test]
fn toml_output_drops_nulls_with_a_warning() {
    let (bytes, warnings) =
        render(&json!({"a": 1, "b": null}), Format::Toml, Style::default()).unwrap();
    assert_eq!(String::from_utf8(bytes).unwrap().trim(), "a = 1");
    assert_eq!(warnings.len(), 1);
}

#[test]
fn xml_round_trip_keeps_attributes_and_lists() {
    let src = br#"<shop id="7"><item>a</item><item>b &amp; c</item></shop>"#;
    let v = parse(src, Format::Xml).unwrap();
    assert_eq!(v, json!({"shop": {"@id": "7", "item": ["a", "b & c"]}}));
    let (out, _) = render(&v, Format::Xml, Style::default()).unwrap();
    assert_eq!(parse(&out, Format::Xml).unwrap(), v);
}

#[test]
fn query_uses_jq_syntax() {
    let v = json!({"people": [{"name": "Ada", "age": 36}, {"name": "Lin", "age": 29}]});
    let out = query(&v, ".people[] | select(.age > 30) | .name").unwrap();
    assert_eq!(out, vec![json!("Ada")]);
    assert_eq!(query(&v, ".people | length").unwrap(), vec![json!(2)]);
    assert!(query(&v, ".people[").is_err());
}

#[test]
fn merge_is_deep_and_arrays_replace_by_default() {
    let mut a = json!({"x": {"p": 1, "q": 2}, "l": [1]});
    merge(&mut a, json!({"x": {"q": 3}, "l": [2]}), false);
    assert_eq!(a, json!({"x": {"p": 1, "q": 3}, "l": [2]}));
    merge(&mut a, json!({"l": [3]}), true);
    assert_eq!(a["l"], json!([2, 3]));
}

#[test]
fn diff_reports_paths() {
    let changes = diff(
        &json!({"a": 1, "b": [1, 2], "c": 3}),
        &json!({"a": 2, "b": [1], "d": 4}),
    );
    let got: Vec<(&str, &str)> = changes.iter().map(|c| (c.path.as_str(), c.kind)).collect();
    assert_eq!(
        got,
        vec![
            (".a", "changed"),
            (".b[1]", "removed"),
            (".c", "removed"),
            (".d", "added")
        ]
    );
}

#[test]
fn json_schema() {
    let schema =
        json!({"type": "object", "required": ["a"], "properties": {"a": {"type": "integer"}}});
    assert!(
        validate_schema(&json!({"a": 1}), &schema)
            .unwrap()
            .is_empty()
    );
    assert!(
        !validate_schema(&json!({"a": "x"}), &schema)
            .unwrap()
            .is_empty()
    );
    assert!(validate_schema(&json!({}), &json!({"type": "nope"})).is_err());
}

#[test]
fn yaml_quotes_what_older_parsers_would_misread() {
    let v = serde_json::json!({"on": true, "yes": "no", "date": "2026-11-06", "at": "1:30", "name": "deft"});
    let (bytes, _) = render(&v, Format::Yaml, Style::default()).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.contains("'on': true"), "{text}");
    assert!(text.contains("'yes': 'no'"), "{text}");
    assert!(text.contains("date: '2026-11-06'"), "{text}");
    assert!(text.contains("at: '1:30'"), "{text}");
    assert!(text.contains("name: deft"), "{text}");
    assert_eq!(parse(text.as_bytes(), Format::Yaml).unwrap(), v);
}

#[test]
fn vcards_read_folded_escaped_and_old_style_lines() {
    let text = "BEGIN:VCARD\r\nVERSION:2.1\r\nN;CHARSET=UTF-8;ENCODING=QUOTED-PRINTABLE:=E5=BC=A0;=E4=B8=89;;;\r\nTEL;CELL:+1 555\r\nitem1.EMAIL:a@b.c\r\nNOTE:one\\, two\r\n  more\r\nEND:VCARD\r\n";
    let c = &vcard::parse(text)[0];
    assert_eq!(c.name, "三 张");
    assert_eq!(c.phones, ["+1 555"]);
    assert_eq!(c.emails, ["a@b.c"]);
    assert_eq!(c.note, "one, two more");
}

#[test]
fn vcards_survive_csv_and_back() {
    let csv = b"Name,Company,Phone,Email\nAda Lovelace,Analytical,+1 555; +1 666,ada@x.io\n";
    let contacts = vcard::from_csv(csv).unwrap();
    assert_eq!(contacts[0].phones.len(), 2);
    let back = vcard::parse(&vcard::write(&contacts));
    assert_eq!(back, contacts);
}

#[test]
fn events_keep_their_times_and_zones() {
    let text = "BEGIN:VEVENT\nDTSTART;TZID=Europe/Oslo:20261106T090000\nDTEND;TZID=Europe/Oslo:20261106T103000\nSUMMARY:Review\\, Q4\nEND:VEVENT\nBEGIN:VEVENT\nDTSTART;VALUE=DATE:20261105\nSUMMARY:Holiday\nEND:VEVENT\n";
    let events = ical::parse(text);
    assert_eq!(events[0].summary, "Holiday");
    assert_eq!(events[0].start, "2026-11-05");
    assert_eq!(events[1].start, "2026-11-06 09:00");
    assert_eq!(events[1].timezone, "Europe/Oslo");
    assert_eq!(events[1].summary, "Review, Q4");
    let back = ical::parse(&ical::write(&events).unwrap());
    assert_eq!(back, events);
}

#[test]
fn losses_are_named() {
    let lost = |t: &str, f| lost_on_reading(t.as_bytes(), f);
    assert_eq!(lost("a: 1 # why\n", Format::Yaml).len(), 1);
    assert!(lost("a: 'x # y'\nb: c#d\n", Format::Yaml).is_empty());
    assert_eq!(lost("version: 1.10\n", Format::Yaml).len(), 1);
    assert!(lost("version: 1.15\nname: v1.10a\n", Format::Yaml).is_empty());
    assert!(lost("<a><b>x</b> <c>y</c></a>", Format::Xml).is_empty());
}

#[test]
fn a_diff_of_lists_gives_jq_paths() {
    let d = diff(&json!([{"id": 1}]), &json!([{"id": 2}]));
    assert_eq!(d[0].path, ".[0].id");
}
