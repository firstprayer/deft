use super::*;
use serde_json::json;

fn yaml(v: &Value) -> String {
    to_yaml(v, None, &HashMap::new())
}

#[test]
fn quotes_what_yaml_readers_would_misread() {
    let v = json!({"t": "10:32:00.555", "d": "2026-11-07", "b": "yes", "o": "0755",
        "m": "1e3", "e": "", "x": "plain text", "c": "a: b", "h": "#x"});
    let out = yaml(&v);
    for line in [
        "t: '10:32:00.555'",
        "d: '2026-11-07'",
        "b: 'yes'",
        "o: '0755'",
        "m: '1e3'",
        "e: ''",
        "x: plain text",
        "c: 'a: b'",
        "h: '#x'",
    ] {
        assert!(out.contains(line), "{line} in {out}");
    }
}

#[test]
fn writes_cjk_and_emoji_without_panicking() {
    let v = json!({"name": "関西    ↓詳しいプロ↓", "f": "%Y年%m月%e日 %H:%M", "w": "⚠️ `#[x]` must\nbe so.\n"});
    let out = yaml(&v);
    assert!(out.contains("name: 関西    ↓詳しいプロ↓"));
    assert!(
        out.contains("w: |\n  ⚠️ `#[x]` must\n  be so.\n"),
        "{out:?}"
    );
}

#[test]
fn floats_read_as_floats_in_yaml_1_1() {
    let v = json!({"a": 1e20, "b": 1.5, "c": 3});
    let out = yaml(&v);
    assert!(out.contains("a: 1.0e+20"), "{out}");
    assert!(out.contains("b: 1.5") && out.contains("c: 3"));
}

#[test]
fn nested_lists_and_maps_and_comments() {
    let v = json!({"spec": {"ports": [{"port": 80, "name": "http"}, 5], "empty": []}});
    let mut c = Comments::default();
    c.at.insert(
        vec![Seg::Key("spec".into()), Seg::Key("ports".into())],
        Note {
            before: vec![" the ports".into()],
            after: Some(" two".into()),
            ..Note::default()
        },
    );
    let out = to_yaml(&v, Some(&c), &HashMap::new());
    assert_eq!(
        out,
        "spec:\n  # the ports\n  ports: # two\n  - port: 80\n    name: http\n  - 5\n  empty: []\n"
    );
}

#[test]
fn nan_and_inf_are_yaml_floats() {
    let mut floats = HashMap::new();
    floats.insert(vec![Seg::Key("a".into())], f64::NAN);
    floats.insert(vec![Seg::Key("b".into())], f64::NEG_INFINITY);
    let out = to_yaml(&json!({"a": null, "b": null, "c": null}), None, &floats);
    assert_eq!(out, "a: .nan\nb: -.inf\nc: null\n");
}
