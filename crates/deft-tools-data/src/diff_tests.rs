use super::*;
use serde_json::json;

#[test]
fn an_inserted_list_item_is_one_change() {
    let a = json!({"files": ["a", "b", "c", "d"]});
    let b = json!({"files": ["a", "x", "b", "c", "d"]});
    let d = diff(&a, &b);
    assert_eq!(d.len(), 1);
    assert_eq!((d[0].path.as_str(), d[0].kind), (".files[1]", "added"));
}

#[test]
fn packages_are_matched_by_name_and_version() {
    let pkg =
        |n: &str, v: &str, deps: &[&str]| json!({"name": n, "version": v, "dependencies": deps});
    let a = json!({"package": [pkg("a", "1", &[]), pkg("b", "1", &["a"]), pkg("b", "2", &[]), pkg("c", "1", &[])]});
    let b = json!({"package": [pkg("a", "1", &[]), pkg("aa", "1", &[]), pkg("b", "1", &["a", "aa"]), pkg("b", "2", &[]), pkg("c", "1", &[])]});
    let d = diff(&a, &b);
    let got: Vec<(&str, &str)> = d.iter().map(|c| (c.path.as_str(), c.kind)).collect();
    assert_eq!(
        got,
        vec![
            (".package[2].dependencies[1]", "added"),
            (".package[1]", "added")
        ]
    );
}

#[test]
fn kubernetes_objects_are_matched_by_kind_and_name() {
    let obj = |k: &str, n: &str, r: i64| json!({"kind": k, "metadata": {"name": n}, "spec": {"replicas": r}});
    let a = json!([obj("Service", "web", 1), obj("Deployment", "web", 1)]);
    let b = json!([obj("Deployment", "web", 3), obj("Service", "web", 1)]);
    let d = diff(&a, &b);
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].path, ".[0].spec.replicas");
}

#[test]
fn equal_documents_have_no_changes() {
    let v = json!({"a": [1, {"b": 2}]});
    assert!(diff(&v, &v).is_empty());
    assert_eq!(diff(&json!(1), &json!(2))[0].path, ".");
}
