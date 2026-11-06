//! The top-level help is the interface agents read first. Changes to it should
//! be deliberate, reviewed as a diff with `cargo insta review`. Per-command help
//! is checked by the contract tests instead, so adding an option touches one file.

use deft_cli::help;
use deft_ops::registry;

#[test]
fn top_and_group_help() {
    insta::assert_snapshot!("top", help::top());
    for g in &registry().groups {
        insta::assert_snapshot!(format!("group_{}", g.name), help::group(g.name));
    }
}

#[test]
fn reference_topics() {
    insta::assert_snapshot!("common", help::common());
    insta::assert_snapshot!("values", help::values());
    insta::assert_snapshot!("errors", help::errors());
}
