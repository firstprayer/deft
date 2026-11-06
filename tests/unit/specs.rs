//! Checks on the command specs themselves, generated from the registry. They run
//! in-process and catch mistakes the moment an operation file is added.

use deft_core::spec::Accept;
use deft_ops::registry;
use deft_testkit::shell_words;
use std::collections::{HashMap, HashSet};

fn tokens(text: &str) -> usize {
    text.len().div_ceil(4)
}

#[test]
fn specs_are_well_formed() {
    let reg = registry();
    let mut problems = Vec::new();
    for (dir, stem, op) in deft_ops::generated::SOURCES {
        let id = format!("{dir}.{}", stem.replace('_', "-"));
        if op.id != id {
            problems.push(format!("{}: id should be {id} (from its file path)", op.id));
        }
    }
    let groups: HashSet<&str> = reg.groups.iter().map(|g| g.name).collect();
    let mut kinds: HashMap<&str, (&str, &str)> = HashMap::new();
    let mut seen_paths: HashMap<String, &str> = HashMap::new();
    let reserved: Vec<&str> = deft_cli::args::COMMON.iter().map(|c| c.0).collect();
    for op in &reg.ops {
        let id = op.id;
        if op.summary.is_empty() || op.summary.len() > 64 {
            problems.push(format!(
                "{id}: summary must be 1-64 chars, is {}",
                op.summary.len()
            ));
        }
        if op.summary.ends_with('.') || !op.summary.chars().next().is_some_and(char::is_uppercase) {
            problems.push(format!(
                "{id}: summary starts uppercase, no trailing period"
            ));
        }
        if !groups.contains(op.group()) {
            problems.push(format!("{id}: no group {}", op.group()));
        }
        if op.examples.is_empty() {
            problems.push(format!("{id}: needs at least one example"));
        }
        for path in op.paths() {
            let (g, _) = path.split_once(' ').unwrap_or(("", ""));
            if !groups.contains(g) {
                problems.push(format!("{id}: mount {path:?} is in no group"));
            }
            if let Some(other) = seen_paths.insert(path.clone(), id) {
                problems.push(format!("path {path:?} claimed by {id} and {other}"));
            }
        }
        for p in op.params {
            if p.help.is_empty() {
                problems.push(format!("{id}: --{} has no help", p.name));
            }
            if p.name.contains('_') || p.name != p.name.to_ascii_lowercase() {
                problems.push(format!("{id}: --{} must be kebab-case", p.name));
            }
            if reserved.contains(&p.name) {
                problems.push(format!("{id}: --{} shadows a common option", p.name));
            }
            if let Some(d) = p.default
                && let Err(e) = p.kind.parse(p.name, d)
            {
                problems.push(format!("{id}: default of --{}: {e}", p.name));
            }
            match kinds.get(p.name) {
                Some((family, owner)) if *family != p.kind.family() => problems.push(format!(
                    "--{} is {} in {owner} but {} in {id}",
                    p.name,
                    family,
                    p.kind.family()
                )),
                _ => {
                    kinds.insert(p.name, (p.kind.family(), id));
                }
            }
        }
        for edge in op.converts {
            for f in edge.to {
                if !op.output.formats.contains(f) {
                    problems.push(format!("{id}: edge target {f} missing from output formats"));
                }
            }
            for f in edge.from {
                if !op.input.accepts(*f) {
                    problems.push(format!("{id}: edge source {f} not accepted as input"));
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn examples_parse_and_reach_their_operation() {
    let reg = registry();
    for op in &reg.ops {
        for example in op.examples {
            let w = shell_words(example);
            assert_eq!(w[0], "deft", "{example}");
            let found = reg
                .resolve(&w[1], &w[2])
                .unwrap_or_else(|| panic!("{example}: path does not resolve"));
            assert_eq!(found.op.id, op.id, "{example}");
            let parsed = deft_cli::args::parse(&w[3..], op.params)
                .unwrap_or_else(|e| panic!("{example}: {e}"));
            assert!(
                !parsed.inputs.is_empty() || op.input.accept == Accept::Nothing,
                "{example}: no input"
            );
        }
    }
}

#[test]
fn help_stays_within_token_budgets() {
    let reg = registry();
    let top = deft_cli::help::top();
    assert!(tokens(&top) <= 400, "top help is {} tokens", tokens(&top));
    for g in &reg.groups {
        let text = deft_cli::help::group(g.name);
        assert!(
            tokens(&text) <= 600,
            "group {} help is {} tokens",
            g.name,
            tokens(&text)
        );
    }
    for (path, op) in reg.all_paths() {
        let text = deft_cli::help::command(op, &path);
        assert!(
            tokens(&text) <= 450,
            "{path} help is {} tokens",
            tokens(&text)
        );
        for other in op.paths() {
            if other != path {
                assert!(
                    text.contains(&format!("deft {other}")),
                    "{path} help should mention {other}"
                );
            }
        }
    }
    let skill = deft_cli::top::skill_text();
    assert!(tokens(&skill) <= 700, "skill is {} tokens", tokens(&skill));
}

#[test]
fn schemas_are_valid_and_conversions_resolve() {
    let reg = registry();
    for op in &reg.ops {
        let s = deft_cli::top::op_schema(op);
        assert_eq!(s["title"], op.id);
        for p in op.params {
            assert!(
                s["properties"][p.name].is_object(),
                "{}: --{}",
                op.id,
                p.name
            );
        }
        for edge in op.converts {
            for &from in edge.from {
                for &to in edge.to {
                    if from != to {
                        assert!(
                            reg.plan_conversion(from, to).is_some(),
                            "{}: {from} -> {to}",
                            op.id
                        );
                    }
                }
            }
        }
    }
}

/// `find_queries.txt`: what people type, against what they should get.
#[test]
fn find_answers_what_people_type() {
    let reg = registry();
    let mut missed = vec![];
    for line in include_str!("find_queries.txt").lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (query, want) = line.split_once('|').expect("query | command");
        let (query, want) = (query.trim(), want.trim());
        let hits = reg.find(query);
        let found = if want == "-" {
            !reg.unknown_job(query).is_empty()
        } else {
            let (group, command) = want.split_once(' ').expect("group command");
            let op = reg.resolve(group, command).expect(want).op.id;
            hits.iter().take(3).any(|h| h.1.id == op)
        };
        if !found {
            let got: Vec<_> = hits.iter().take(3).map(|h| h.0.as_str()).collect();
            missed.push(format!("{query}: wanted {want}, got {got:?}"));
        }
    }
    assert!(missed.is_empty(), "{}", missed.join("\n"));
}
