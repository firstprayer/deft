use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "md.toc",
    summary: "Insert a table of contents from the headings",
    details: &["It replaces a <!-- toc --> line, or goes after the first heading."],
    input: Input::each(&[Format::Md], "md"),
    output: Output::one(&[Format::Md], "md"),
    params: &[param(
        "depth",
        Kind::Int { min: 1, max: 6 },
        "Deepest heading level",
    )
    .default("3")],
    examples: &["deft md toc notes.md --depth 2 -o with-toc.md"],
    tags: &["contents", "outline", "navigation", "headings"],
    result: &["entries"],
    suffix: Some("toc"),
    run,
    ..Op::DEFAULT
};

/// GitHub's anchor for a heading.
fn slug(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            _ => None,
        })
        .collect()
}

fn run(job: &Job) -> Result<OpResult> {
    let md = String::from_utf8_lossy(job.input().bytes()?).to_string();
    let depth = job.params.int("depth") as usize;
    let mut entries = Vec::new();
    let mut first = None;
    let mut fenced = false;
    let mut seen = std::collections::HashMap::new();
    for (n, line) in md.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
        }
        let level = line.bytes().take_while(|b| *b == b'#').count();
        if fenced || level == 0 || level > 6 || line.as_bytes().get(level) != Some(&b' ') {
            continue;
        }
        let text = line[level..].trim().trim_end_matches('#').trim();
        if first.is_none() {
            first = Some(n);
            if level == 1 {
                continue;
            }
        }
        if level <= depth {
            let base = slug(text);
            let count: &mut usize = seen.entry(base.clone()).or_default();
            let anchor = if *count == 0 {
                base
            } else {
                format!("{base}-{count}")
            };
            *count += 1;
            entries.push(format!(
                "{}- [{text}](#{anchor})",
                "  ".repeat(level.saturating_sub(2))
            ));
        }
    }
    if entries.is_empty() {
        return Ok(OpResult::one(Produced::bytes(md.into_bytes(), Format::Md))
            .fact("entries", 0)
            .warn("no headings to list"));
    }
    let toc = entries.join("\n");
    let mut lines: Vec<String> = md.lines().map(String::from).collect();
    if let Some(i) = lines
        .iter()
        .position(|l| l.trim().eq_ignore_ascii_case("<!-- toc -->"))
    {
        lines[i] = toc;
    } else {
        let at = first.map_or(0, |n| n + 1);
        lines.insert(at, format!("\n{toc}"));
    }
    let n = entries.len();
    let mut out = lines.join("\n");
    out.push('\n');
    Ok(OpResult::one(Produced::bytes(out.into_bytes(), Format::Md))
        .fact("entries", n)
        .note(format!("{n} entries")))
}
