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
