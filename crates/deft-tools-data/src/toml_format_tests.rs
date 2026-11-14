use super::*;

#[test]
fn keeps_comments_and_spellings() {
    let src = "# top\n  name   =  \"x\"   # the name\n\n\n\nn=nan\ninf = -inf\n[t]   # table\n    # inside\n    a=0x1F\n";
    let out = format(src, false).unwrap();
    assert_eq!(
        out,
        "# top\nname = \"x\" # the name\n\nn = nan\ninf = -inf\n\n[t] # table\n# inside\na = 0x1F\n"
    );
}

#[test]
fn sorts_keys_and_tables_with_their_comments() {
    let src = "b = 1\n# about a\na = 2\n\n[z]\nx = 1\n\n[m]\n# m's key\ny = 2\n";
    let out = format(src, true).unwrap();
    assert_eq!(
        out,
        "# about a\na = 2\nb = 1\n\n[m]\n# m's key\ny = 2\n\n[z]\nx = 1\n"
    );
}

#[test]
fn sorting_turns_dotted_keys_into_sections_beside_other_sections() {
    let src =
        "[assign]\nz = 1\nwarn.enable = true\ndep = { b = 1, a = 2 }\n\n[assign.adhoc]\nx = 1\n";
    let out = format(src, true).unwrap();
    assert_eq!(
        out,
        "[assign]\ndep = { a = 2, b = 1 }\nz = 1\n\n[assign.adhoc]\nx = 1\n\n[assign.warn]\nenable = true\n"
    );
}
