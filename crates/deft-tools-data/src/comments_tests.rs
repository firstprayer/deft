use super::*;

fn before(c: &Comments, path: &[Seg]) -> Vec<String> {
    c.get(path).map(|n| n.before.clone()).unwrap_or_default()
}

#[test]
fn yaml_comments_attach_to_the_key_below_or_on_their_line() {
    let text = "# top\nimage:\n  # the registry\n  registry: docker.io # where\n  tag: \"a # b\"\nlist:\n  # first\n  - a\n  - |\n    # not a comment\n    x\n# end\n";
    let c = yaml_extras(text, 1).0.unwrap();
    let key = |k: &str| Seg::Key(k.into());
    assert_eq!(before(&c, &[key("image")]), vec![" top"]);
    assert_eq!(
        before(&c, &[key("image"), key("registry")]),
        vec![" the registry"]
    );
    let n = c.get(&[key("image"), key("registry")]).unwrap();
    assert_eq!(n.after.as_deref(), Some(" where"));
    assert!(c.get(&[key("image"), key("tag")]).is_none());
    assert_eq!(before(&c, &[key("list"), Seg::Index(0)]), vec![" first"]);
    assert_eq!(c.get(&[]).unwrap().end, vec![" end"]);
    assert_eq!(c.count(), 5);
}

#[test]
fn toml_comments_cover_keys_tables_and_array_items() {
    let text = "# head\nname = \"x\" # the name\n\n# deps\n[dependencies]\nserde = \"1\"\n\n[[bin]]\n# first bin\nname = \"a\"\n\nlist = [\n  # one\n  1, # after one\n  2,\n]\n";
    let c = toml_comments(text).unwrap();
    let key = |k: &str| Seg::Key(k.into());
    assert_eq!(before(&c, &[key("name")]), vec![" head"]);
    assert_eq!(
        c.get(&[key("name")]).unwrap().after.as_deref(),
        Some(" the name")
    );
    assert_eq!(before(&c, &[key("dependencies")]), vec![" deps"]);
    let bin = [key("bin"), Seg::Index(0), key("name")];
    assert_eq!(before(&c, &bin), vec![" first bin"]);
    let list = [key("bin"), Seg::Index(0), key("list"), Seg::Index(0)];
    assert_eq!(before(&c, &list), vec![" one"]);
    assert_eq!(c.get(&list).unwrap().after.as_deref(), Some(" after one"));
}

#[test]
fn comments_of_missing_nodes_move_to_their_parent() {
    let mut c = Comments::default();
    c.note(&[Seg::Key("a".into()), Seg::Key("gone".into())], 0)
        .before
        .push(" lost".into());
    c.rehome(&serde_json::json!({"a": {"b": 1}}));
    assert_eq!(c.get(&[Seg::Key("a".into())]).unwrap().end, vec![" lost"]);
}
