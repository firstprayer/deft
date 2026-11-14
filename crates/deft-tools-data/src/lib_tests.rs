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
    let check = |v| validate_schema(&v, &schema, None).unwrap().problems;
    assert!(check(json!({"a": 1})).is_empty());
    assert!(!check(json!({"a": "x"})).is_empty());
    assert!(validate_schema(&json!({}), &json!({"type": "nope"}), None).is_err());
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
fn losses_are_named() {
    let lost = |t: &str, f| lost_on_reading(t.as_bytes(), f);
    let comments = |t: &str| read(t.as_bytes(), Format::Yaml).unwrap().comments.count();
    assert_eq!(comments("a: 1 # why\n"), 1);
    assert_eq!(comments("a: 'x # y'\nb: c#d\n"), 0);
    assert_eq!(lost("version: 1.10\n", Format::Yaml).len(), 1);
    assert!(lost("version: 1.15\nname: v1.10a\n", Format::Yaml).is_empty());
    assert!(lost("<a><b>x</b> <c>y</c></a>", Format::Xml).is_empty());
    assert_eq!(lost("a: &b\n  x: 1\nc: *b\n", Format::Yaml).len(), 1);
    assert!(lost("a: b & c\nd: 2 * 3\n", Format::Yaml).is_empty());
    assert_eq!(lost("{\"n\":123456789012345678901}", Format::Json).len(), 1);
    assert!(lost("{\"n\":\"123456789012345678901\"}", Format::Json).is_empty());
    assert!(lost("{\"n\":12345678901234567890}", Format::Json).is_empty());
}

#[test]
fn a_diff_of_lists_gives_jq_paths() {
    let d = diff(&json!([{"id": 1}]), &json!([{"id": 2}]));
    assert_eq!(d[0].path, ".[0].id");
}

#[test]
fn yaml_merge_key_lists_apply_inside_merged_mappings() {
    let src = b"r: &r {restart: always}\np: &p {pull: never}\nd: &d\n  <<: [*r, *p]\n  image: x\nsvc:\n  <<: *d\n  pull: always\n";
    let v = parse(src, Format::Yaml).unwrap();
    assert_eq!(
        v["svc"],
        json!({"restart": "always", "pull": "always", "image": "x"})
    );
    assert!(v["d"].get("<<").is_none());
}

#[test]
fn yaml_tags_are_kept_as_text_and_reported() {
    let doc = read(
        b"emoji: !!python/name:material.emoji.twemoji\nn: !Ref Bucket\nslug: !!python/object/apply:s.slugify\n  kwargs: {case: lower}\n",
        Format::Yaml,
    )
    .unwrap();
    assert_eq!(
        doc.value["emoji"],
        json!("!!python/name:material.emoji.twemoji")
    );
    assert_eq!(doc.value["n"], json!("Bucket"));
    assert!(
        doc.warnings[0].contains("not kept on 3 values"),
        "{:?}",
        doc.warnings
    );
}

#[test]
fn jsonc_is_read_for_converting_and_named_when_validating() {
    let src = b"{\n  // editor\n  \"a\": 1, /* x */\n  \"b\": [1, 2,],\n}\n";
    let e = read(src, Format::Json).unwrap_err();
    assert!(
        e.message
            .contains("comments or trailing commas are not allowed in JSON (line 2"),
        "{}",
        e.message
    );
    let doc = read_lenient(src, Format::Json).unwrap();
    assert_eq!(doc.value, json!({"a": 1, "b": [1, 2]}));
    assert!(doc.warnings[0].contains("JSONC"));
}

#[test]
fn toml_comments_survive_a_trip_to_yaml_and_back() {
    let src = b"# the package\n[package]\nname = \"x\" # its name\n\n# features\n[features]\ndefault = []\n";
    let doc = read(src, Format::Toml).unwrap();
    let (yaml, warnings) = render_doc(&doc, Format::Yaml, Style::default()).unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    let yaml = String::from_utf8(yaml).unwrap();
    assert_eq!(
        yaml,
        "# the package\npackage:\n  name: x # its name\n# features\nfeatures:\n  default: []\n"
    );
    let back = read(yaml.as_bytes(), Format::Yaml).unwrap();
    let (toml, _) = render_doc(&back, Format::Toml, Style::default()).unwrap();
    let toml = String::from_utf8(toml).unwrap();
    for c in ["# the package", "# its name", "# features"] {
        assert!(toml.contains(c), "{c} in {toml}");
    }
}

#[test]
fn nan_and_inf_survive_into_yaml_and_toml_and_are_reported_for_json() {
    let doc = read(b"a = nan\nb = -inf\n", Format::Toml).unwrap();
    let (toml, w) = render_doc(&doc, Format::Toml, Style::default()).unwrap();
    assert_eq!(String::from_utf8(toml).unwrap(), "a = nan\nb = -inf\n");
    assert!(w.is_empty());
    let (_, w) = render_doc(&doc, Format::Json, Style::default()).unwrap();
    assert!(w[0].contains("no nan or inf"), "{w:?}");
}

#[test]
fn xml_keeps_keys_that_are_not_names_and_empty_lists() {
    let (out, w) = render(
        &json!({"r": {"+1": 4, "runWarnings": [], "a/b.py": "x"}}),
        Format::Xml,
        Style::default(),
    )
    .unwrap();
    let out = String::from_utf8(out).unwrap();
    assert!(out.contains("<item key=\"+1\">4</item>"), "{out}");
    assert!(out.contains("<runWarnings/>"));
    assert!(out.contains("<item key=\"a/b.py\">x</item>"));
    assert!(w[0].contains("2 keys are not XML names"));
}

#[test]
fn plist_keys_stay_with_their_values() {
    let src = br#"<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleName</key><string>IINA</string><key>Types</key><array><dict><key>n</key><integer>3</integer><key>ok</key><true/></dict></array></dict></plist>"#;
    let v = parse(src, Format::Xml).unwrap();
    assert_eq!(
        v,
        json!({"plist": {"@version": "1.0", "dict": {"CFBundleName": "IINA", "Types": [{"n": 3, "ok": true}]}}})
    );
}

#[test]
fn xml_comments_are_reported_as_lost() {
    let doc = read(b"<!-- why --><rss><channel/></rss>", Format::Xml).unwrap();
    assert_eq!(doc.warnings, vec!["comments are not kept"]);
}

#[test]
fn mixed_xml_content_keeps_its_order() {
    let v = parse(b"<p><b>DOI:</b> <a>10.7554</a></p>", Format::Xml).unwrap();
    assert_eq!(
        v,
        json!({"p": {"#content": [{"b": "DOI:"}, " ", {"a": "10.7554"}]}})
    );
    // Indentation between elements is not text.
    let v = parse(b"<p>\n  <b>x</b>\n  <a>y</a>\n</p>", Format::Xml).unwrap();
    assert_eq!(v, json!({"p": {"b": "x", "a": "y"}}));
    let v = parse(b"<p>Some <b>bold</b> <i>and</i> text</p>", Format::Xml).unwrap();
    assert_eq!(
        v,
        json!({"p": {"#content": ["Some ", {"b": "bold"}, " ", {"i": "and"}, " text"]}})
    );
}

#[test]
fn json_floats_keep_their_last_digit() {
    let v = parse(b"[90.95299999999995, 14.116999999999999]", Format::Json).unwrap();
    assert_eq!(v.to_string(), "[90.95299999999995,14.116999999999999]");
}

#[test]
fn a_repeated_yaml_key_takes_the_last_value_with_a_warning() {
    let doc = read(b"a:\n  x: 1\n  x: 2\nb: [1]\n", Format::Yaml).unwrap();
    assert_eq!(doc.value, json!({"a": {"x": 2}, "b": [1]}));
    assert!(
        doc.warnings[0].contains("the last value is used"),
        "{:?}",
        doc.warnings
    );
}
