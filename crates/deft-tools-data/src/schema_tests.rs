use super::*;
use serde_json::json;

#[test]
fn remote_references_are_reported_not_fetched() {
    let schema = json!({
        "properties": {
            "eslintConfig": {"$ref": "https://json.schemastore.org/eslintrc.json"},
            "name": {"type": "string"}
        }
    });
    let ok = validate(
        &json!({"name": "x", "eslintConfig": {"any": 1}}),
        &schema,
        None,
    )
    .unwrap();
    assert!(ok.problems.is_empty());
    assert_eq!(
        ok.unchecked,
        vec!["https://json.schemastore.org/eslintrc.json"]
    );
    let bad = validate(&json!({"name": 3}), &schema, None).unwrap();
    assert!(
        bad.problems.iter().any(|p| p.contains("/name")),
        "{:?}",
        bad.problems
    );
}

#[test]
fn look_ahead_patterns_are_checked() {
    let schema = json!({
        "type": "object",
        "properties": {"repo": {"type": "string", "pattern": "^(?!(?:meta|local)$).*$"}},
        "patternProperties": {"^(?!tag|category)[a-z]+_n$": {"type": "integer"}}
    });
    let check = |v| validate(&v, &schema, None).unwrap().problems;
    assert!(check(json!({"repo": "https://x"})).is_empty());
    assert!(!check(json!({"repo": "meta"})).is_empty());
    // "tag" is not matched by the pattern, so it may hold a string.
    assert!(check(json!({"tag_n": "x", "count_n": 2})).is_empty());
    assert!(!check(json!({"count_n": "two"})).is_empty());
}

#[test]
fn references_inside_the_schema_resolve() {
    let schema = json!({
        "$id": "https://example.com/s.json",
        "definitions": {"n": {"type": "integer"}},
        "properties": {"a": {"$ref": "#/definitions/n"}, "b": {"$ref": "https://example.com/s.json#/definitions/n"}}
    });
    let r = validate(&json!({"a": 1, "b": "x"}), &schema, None).unwrap();
    assert!(r.unchecked.is_empty());
    assert!(
        r.problems.iter().any(|p| p.contains("/b")),
        "{:?}",
        r.problems
    );
}
