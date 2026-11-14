use super::*;
use serde_json::json;

fn toml(v: &Value) -> String {
    to_toml(v.as_object().unwrap(), None, &HashMap::new())
}

#[test]
fn plain_keys_come_before_tables_and_lists_of_tables() {
    let v =
        json!({"name": "x", "dep": {"serde": "1"}, "n": 2, "bin": [{"name": "a"}, {"name": "b"}]});
    let out = toml(&v);
    assert_eq!(
        out,
        "name = \"x\"\nn = 2\n\n[dep]\nserde = \"1\"\n\n[[bin]]\nname = \"a\"\n\n[[bin]]\nname = \"b\"\n"
    );
    let back: toml::Value = toml::from_str(&out).unwrap();
    assert_eq!(back["bin"][1]["name"].as_str(), Some("b"));
}

#[test]
fn odd_keys_and_text_are_quoted() {
    let v = json!({"a b": "line\nnext \"q\"", "k": [1, {"x": "y"}], "e": {}});
    let out = toml(&v);
    let back: toml::Value = toml::from_str(&out).unwrap();
    assert_eq!(back["a b"].as_str(), Some("line\nnext \"q\""));
    assert_eq!(back["k"][1]["x"].as_str(), Some("y"));
    assert!(back["e"].as_table().unwrap().is_empty());
}

#[test]
fn comments_are_written_next_to_their_keys() {
    let v = json!({"a": 1, "t": {"b": [1, 2]}});
    let mut c = Comments::default();
    c.at.insert(
        vec![Seg::Key("a".into())],
        Note {
            before: vec![" one".into()],
            after: Some(" yes".into()),
            ..Note::default()
        },
    );
    c.at.insert(
        vec![Seg::Key("t".into())],
        Note {
            before: vec![" table".into()],
            ..Note::default()
        },
    );
    c.at.insert(
        vec![Seg::Key("t".into()), Seg::Key("b".into()), Seg::Index(1)],
        Note {
            before: vec![" second".into()],
            ..Note::default()
        },
    );
    let out = to_toml(v.as_object().unwrap(), Some(&c), &HashMap::new());
    assert_eq!(
        out,
        "# one\na = 1 # yes\n\n# table\n[t]\nb = [\n    1,\n    # second\n    2,\n]\n"
    );
    toml::from_str::<toml::Value>(&out).unwrap();
}

#[test]
fn multiline_strings_with_quotes_round_trip() {
    for s in ["a\n\"\"\"b", "end\"", "x\n\"\"", "back\\slash\n"] {
        let out = toml(&json!({ "s": s }));
        let back: toml::Value = toml::from_str(&out).unwrap();
        assert_eq!(back["s"].as_str(), Some(s), "{out}");
    }
}
