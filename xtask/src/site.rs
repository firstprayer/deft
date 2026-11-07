//! `cargo xtask site`: `docs/COMMANDS.md` and `skills/deft/SKILL.md` are built
//! from the compiled registry, so a new command reaches them without anyone
//! editing them. `cargo xtask site --check` fails when one is out of date, and
//! it is part of `cargo xtask check`.
//!
//! `cargo xtask page <template> <output>` fills a page template with the same
//! registry and the eval results. The template is kept outside this repository.

use crate::eval::model::{Case, load_cases};
use crate::eval::report::{latest_results, new_tokens};
use crate::root;
use deft_cli::help;
use deft_core::spec::Op;
use deft_ops::registry::registry;
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::fmt::Write as _;

/// The arm the homepage reports, and the baseline it is compared with.
const ARM: &str = "codex-with-deft";
const BASELINE: &str = "codex-without-deft";

/// Tasks offered in the page's picker. The last one is a task deft lost, kept
/// on purpose so the picker cannot read as a highlight reel.
const PICKS: &[&str] = &[
    "pdf/rotate-one-page",
    "pdf/compress-scan",
    "sheet/ods-to-csv",
    "image/text-on-photo",
    "video/join",
    "audio/compress",
];

/// The search shown before the visitor types anything.
const FIRST_SEARCH: &str = "shrink pdf";

pub fn write(check: bool) -> Result<(), String> {
    let outputs = [
        ("docs/COMMANDS.md", reference()),
        ("skills/deft/SKILL.md", deft_cli::top::skill_text()),
    ];
    for (path, text) in outputs {
        let file = root().join(path);
        if check {
            if std::fs::read_to_string(&file).ok().as_deref() != Some(text.as_str()) {
                return Err(format!("{path} is out of date; run: cargo xtask site"));
            }
            continue;
        }
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        std::fs::write(&file, text).map_err(|e| e.to_string())?;
        println!("wrote {path}");
    }
    Ok(())
}

/// Sums for one set of tasks, with deft and without.
#[derive(Default)]
struct Lift {
    tasks: usize,
    tokens: [u64; 2],
    seconds: [f64; 2],
    failed: [u64; 2],
    passed: [u64; 2],
    used_more_tokens: usize,
}

impl Lift {
    fn add(&mut self, with: &Value, without: &Value) {
        self.tasks += 1;
        for (i, r) in [with, without].into_iter().enumerate() {
            self.tokens[i] += new_tokens(r);
            self.seconds[i] += r["seconds"].as_f64().unwrap_or(0.0);
            self.failed[i] += r["failed_commands"].as_u64().unwrap_or(0);
            self.passed[i] += u64::from(r["passed"] == true);
        }
        self.used_more_tokens += usize::from(new_tokens(with) > new_tokens(without));
    }
}

/// Percent saved by the first number relative to the second; negative is worse.
fn saved(with: f64, without: f64) -> i64 {
    if without == 0.0 {
        return 0;
    }
    ((1.0 - with / without) * 100.0).round() as i64
}

/// Cases that were gaps in the last agent run and have commands now. Their
/// results describe the old product, so they stay out of the published totals
/// until the agents run again; then empty this list.
const CLOSED_SINCE_LAST_RUN: &[&str] = &[
    "image/collage",
    "image/heic-to-jpg",
    "md/to-docx",
    "pdf/fill-form",
    "pdf/ocr-scan",
    "sheet/all-sheets",
    "sheet/filter-rows",
    "video/crop-square",
];

/// Cases removed since the last run. Their results are still part of the totals
/// that run published, so they stay in until the agents run again; then empty
/// this list.
const REMOVED_SINCE_LAST_RUN: &[&str] = &[
    "convert/chain-md-to-png",
    "image/flip",
    "pdf/already-rotated",
    "pdf/empty-password",
    "archive/list",
    "archive/tar-folder",
    "archive/unzip",
    "archive/zip-files",
    "subtitle/count-lines",
    "subtitle/extract-from-video",
    "subtitle/fix-sync",
    "subtitle/srt-to-vtt",
    "subtitle/to-transcript",
    "video/burn-chinese-subtitles",
];

/// The last row of `evals/lift-history.jsonl`: the run the page publishes.
fn published_run() -> Result<Value, String> {
    let path = crate::root().join("evals/lift-history.jsonl");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let last = text.lines().last().ok_or("lift history is empty")?;
    serde_json::from_str(last).map_err(|e| format!("lift history: {e}"))
}

struct Evals {
    cases: usize,
    gaps: usize,
    not_run: usize,
    total: Lift,
    groups: BTreeMap<String, Lift>,
    picks: Vec<Value>,
}

fn evals() -> Result<Evals, String> {
    let cases = load_cases()?;
    let results = latest_results()?;
    let found = |c: &Case, arm: &str| results.get(&format!("{}|{arm}", c.id));
    let mut total = Lift::default();
    let mut groups: BTreeMap<String, Lift> = BTreeMap::new();
    for c in &cases {
        let (Some(with), Some(without)) = (found(c, ARM), found(c, BASELINE)) else {
            continue;
        };
        if c.shape != "gap" && !CLOSED_SINCE_LAST_RUN.contains(&c.id.as_str()) {
            total.add(with, without);
            groups
                .entry(c.group.clone())
                .or_default()
                .add(with, without);
        }
    }
    for id in REMOVED_SINCE_LAST_RUN {
        let result = |arm: &str| results.get(&format!("{id}|{arm}"));
        let (Some(with), Some(without)) = (result(ARM), result(BASELINE)) else {
            return Err(format!("removed case {id} has no result for both arms"));
        };
        let group = id.split('/').next().unwrap_or(id).to_string();
        total.add(with, without);
        groups.entry(group).or_default().add(with, without);
    }
    let mut picks = Vec::new();
    for id in PICKS {
        let c = cases
            .iter()
            .find(|c| c.id == *id)
            .ok_or_else(|| format!("site pick {id} is not an eval case"))?;
        let (Some(with), Some(without)) = (found(c, ARM), found(c, BASELINE)) else {
            return Err(format!("site pick {id} has no result for both arms"));
        };
        picks.push(json!({
            "id": c.id, "title": c.title, "prompt": c.prompt, "reference": c.reference,
            "with": attempt(with), "without": attempt(without),
        }));
    }
    let run = published_run()?;
    let pair = run["pairs"]
        .as_array()
        .and_then(|p| p.iter().find(|p| p["arm"] == ARM))
        .ok_or("lift history has no row for the published arm")?;
    let recorded = pair["tokens_supported"][0].as_f64().unwrap_or(0.0) as u64;
    if total.tokens[0] != recorded {
        return Err(format!(
            "published totals drifted: {} tokens here, {recorded} in the last run; \
             update CLOSED_SINCE_LAST_RUN or record a new run",
            total.tokens[0]
        ));
    }
    let count = |v: &Value| v.as_u64().unwrap_or(0) as usize;
    Ok(Evals {
        cases: count(&run["cases"]),
        gaps: count(&run["gaps"]),
        not_run: count(&run["cases"]) - count(&pair["cases"]),
        total,
        groups,
        picks,
    })
}

fn attempt(r: &Value) -> Value {
    json!({
        "tokens": new_tokens(r),
        "seconds": r["seconds"].as_f64().unwrap_or(0.0).round(),
        "commands": r["commands"],
        "failed": r["failed_commands"],
        "passed": r["passed"],
    })
}

/// Fills `template` and writes the result to `output`.
pub fn write_page(template: &str, output: &str) -> Result<(), String> {
    let template = std::fs::read_to_string(template).map_err(|e| format!("{template}: {e}"))?;
    std::fs::write(output, page(template)?).map_err(|e| format!("{output}: {e}"))?;
    println!("wrote {output}");
    Ok(())
}

fn page(mut html: String) -> Result<String, String> {
    let e = evals()?;
    let t = &e.total;
    let values: Vec<(&str, String)> = vec![
        ("version", help::VERSION.to_string()),
        ("commands", registry().ops.len().to_string()),
        ("groups", registry().groups.len().to_string()),
        ("cases", e.cases.to_string()),
        ("gaps", e.gaps.to_string()),
        ("not_run", e.not_run.to_string()),
        ("supported", t.tasks.to_string()),
        ("pass_with", t.passed[0].to_string()),
        ("pass_without", t.passed[1].to_string()),
        (
            "tokens_saved",
            saved(t.tokens[0] as f64, t.tokens[1] as f64).to_string(),
        ),
        ("time_saved", saved(t.seconds[0], t.seconds[1]).to_string()),
        ("failed_with", t.failed[0].to_string()),
        ("failed_without", t.failed[1].to_string()),
        ("used_more_tokens", t.used_more_tokens.to_string()),
        ("first_search", FIRST_SEARCH.to_string()),
        ("first_hits", first_hits()),
        ("group_rows", group_rows(&e.groups)),
        ("command_list", command_list()),
        (
            "data",
            client_data(&e.picks).to_string().replace('<', "\\u003c"),
        ),
    ];
    for (name, value) in values {
        html = html.replace(&format!("{{{{{name}}}}}"), &value);
    }
    if let Some(i) = html.find("{{") {
        let rest: String = html[i..].chars().take(30).collect();
        return Err(format!("the template has an unknown placeholder: {rest}"));
    }
    Ok(html)
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// The first search, rendered in the page so it works without script.
fn first_hits() -> String {
    let mut s = String::new();
    for (path, op, _) in registry().find(FIRST_SEARCH) {
        let _ = writeln!(
            s,
            "<li><button type=\"button\" data-path=\"{p}\"><code>deft {p}</code><code class=\"sum\">{sum}</code></button></li>",
            p = esc(&path),
            sum = esc(op.summary),
        );
    }
    s
}

fn group_rows(groups: &BTreeMap<String, Lift>) -> String {
    let mut rows: Vec<(&String, &Lift, i64, i64)> = groups
        .iter()
        .map(|(name, l)| {
            let tokens = saved(l.tokens[0] as f64, l.tokens[1] as f64);
            (name, l, tokens, saved(l.seconds[0], l.seconds[1]))
        })
        .collect();
    rows.sort_by_key(|r| -r.2);
    let scale = rows
        .iter()
        .map(|r| r.2.abs().max(r.3.abs()))
        .max()
        .unwrap_or(1)
        .max(1) as f64;
    let cell = |v: i64, what: &str| {
        let (word, class) = match v {
            0 => ("same".to_string(), "flat"),
            v if v > 0 => (format!("{v}% less"), "win"),
            v => (format!("{}% more", -v), "loss"),
        };
        let width = (v.abs() as f64 / scale * 100.0).round();
        format!(
            "<td class=\"{class}\"><span class=\"bar\" style=\"--w:{width}%\"></span><span class=\"num\">{word}<span class=\"vh\"> {what}</span></span></td>"
        )
    };
    let mut s = String::new();
    for (name, l, tokens, seconds) in rows {
        let _ = writeln!(
            s,
            "<tr><th scope=\"row\">{}</th><td class=\"n\">{}</td>{}{}</tr>",
            esc(name),
            l.tasks,
            cell(tokens, "tokens"),
            cell(seconds, "time"),
        );
    }
    s
}

/// What a command needs beyond the binary, as short words.
fn needs(op: &Op) -> Vec<String> {
    let mut v: Vec<String> = op.engines.iter().map(|e| e.to_string()).collect();
    if op.network {
        v.push("network".to_string());
    }
    v
}

fn command_list() -> String {
    let reg = registry();
    let mut s = String::new();
    for g in &reg.groups {
        let _ = writeln!(
            s,
            "<details class=\"group\" open><summary><h3>{}</h3><p>{}</p></summary><ul>",
            esc(g.name),
            esc(g.summary)
        );
        for (command, op) in reg.commands(g.name) {
            let _ = writeln!(
                s,
                "<li><code>{}</code><span>{}</span></li>",
                esc(&command),
                esc(op.summary)
            );
        }
        s.push_str("</ul></details>\n");
    }
    s.push_str("<details class=\"group\" open><summary><h3>Any file</h3><p>Commands outside a group</p></summary><ul>\n");
    for c in help::TOP_COMMANDS {
        let _ = writeln!(
            s,
            "<li><code>{}</code><span>{}</span></li>",
            esc(c.name),
            esc(c.summary)
        );
    }
    s.push_str("</ul></details>\n");
    s
}

/// The data the page's script needs: the search index and the picker.
fn client_data(picks: &[Value]) -> Value {
    let reg = registry();
    let mut ops = Map::new();
    for op in &reg.ops {
        ops.insert(
            op.id.to_string(),
            json!({
                "summary": op.summary,
                "canonical": op.path(),
                "help": help::command(op, &op.path()),
            }),
        );
    }
    json!({
        "ops": ops,
        "search": reg.search_index(),
        "picks": picks,
    })
}

fn cell(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

/// `docs/COMMANDS.md`.
fn reference() -> String {
    let reg = registry();
    let mut s = String::from(
        "# Command reference\n\nGenerated by `cargo xtask site` from the command registry. Do not edit this file; change the operation in `crates/deft-ops/src/ops/`. For options, run `deft <group> <command> --help`.\n\nA command marked ffmpeg or yt-dlp needs that program installed. See [INSTALL.md](INSTALL.md).\n\n## Any file\n\n| Command | What it does | Example |\n|---|---|---|\n",
    );
    for c in help::TOP_COMMANDS {
        let _ = writeln!(
            s,
            "| `deft {}` | {} | `{}` |",
            c.name,
            cell(c.summary),
            cell(c.example)
        );
    }
    for g in &reg.groups {
        let _ = write!(
            s,
            "\n## {}\n\n{}\n\n| Command | What it does | Needs | Example |\n|---|---|---|---|\n",
            g.name, g.summary
        );
        for (command, op) in reg.commands(g.name) {
            let example = op
                .examples
                .first()
                .map(|e| format!("`{}`", cell(e)))
                .unwrap_or_default();
            let _ = writeln!(
                s,
                "| `deft {} {command}` | {} | {} | {example} |",
                g.name,
                cell(op.summary),
                needs(op).join(", "),
            );
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_command_reaches_the_page_and_the_reference() {
        let list = command_list();
        let doc = reference();
        for g in &registry().groups {
            for (command, _) in registry().commands(g.name) {
                assert!(list.contains(&format!("<code>{command}</code>")));
                assert!(doc.contains(&format!("`deft {} {command}`", g.name)));
            }
        }
    }

    #[test]
    fn savings_keep_their_sign() {
        assert_eq!(saved(73.0, 100.0), 27);
        assert_eq!(saved(150.0, 100.0), -50);
        assert_eq!(saved(1.0, 0.0), 0);
    }

    #[test]
    fn text_cannot_break_out_of_the_page() {
        assert_eq!(esc("<a href=\"x\">&"), "&lt;a href=&quot;x&quot;&gt;&amp;");
    }

    /// A `: ` in an unquoted YAML value is a syntax error, and the description
    /// has one, so skill loaders would reject the file.
    #[test]
    fn the_skill_description_is_quoted() {
        let text = deft_cli::top::skill_text();
        let line = text.lines().find(|l| l.starts_with("description: "));
        let value = line.unwrap().trim_start_matches("description: ");
        assert!(value.starts_with('"') && value.ends_with('"'), "{value}");
    }

    #[test]
    fn the_first_search_finds_something() {
        assert!(first_hits().contains("deft pdf compress"));
    }
}
