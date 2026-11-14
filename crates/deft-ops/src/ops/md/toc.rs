use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "md.toc",
    summary: "Insert a table of contents from the headings",
    details: &[
        "It replaces a <!-- toc --> line, or goes after the title.",
        "Running it again replaces the list.",
    ],
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

const OPEN: &str = "<!-- toc -->";
const CLOSE: &str = "<!-- /toc -->";

/// The list of entries for these headings. GitHub numbers repeats across all
/// headings, listed or not, so every heading is counted.
fn entries(headings: &[deft_tools_doc::markdown::Heading], depth: usize) -> Vec<String> {
    let mut anchors = deft_core::text::Anchors::default();
    let mut out = Vec::new();
    let title = has_title(headings);
    let top = headings
        .iter()
        .skip(usize::from(title))
        .map(|h| h.level)
        .min()
        .unwrap_or(1);
    for (i, h) in headings.iter().enumerate() {
        let anchor = anchors.next(&h.plain);
        // A leading title names the document, so it is not an entry.
        if !(title && i == 0) && h.level <= depth {
            out.push(format!(
                "{}- [{}](#{anchor})",
                "  ".repeat(h.level.saturating_sub(top)),
                h.label
            ));
        }
    }
    out
}

/// Whether the first heading is the document's title: a level-one heading,
/// or one of any level or syntax that every later heading is below, as a
/// README's underlined title or an mdBook chapter's "## Title" is.
fn has_title(headings: &[deft_tools_doc::markdown::Heading]) -> bool {
    let Some(first) = headings.first() else {
        return false;
    };
    first.level == 1 || (headings.len() > 1 && headings[1..].iter().all(|h| h.level > first.level))
}

fn run(job: &Job) -> Result<OpResult> {
    let md = String::from_utf8_lossy(job.input().bytes()?).to_string();
    let depth = job.params.int("depth") as usize;
    let headings = deft_tools_doc::markdown::headings(&md);
    let list = entries(&headings, depth);
    if list.is_empty() {
        return Ok(OpResult::one(Produced::bytes(md.into_bytes(), Format::Md))
            .fact("entries", 0)
            .warn("no headings to list"));
    }
    let block = format!("{OPEN}\n{}\n{CLOSE}", list.join("\n"));
    let mut lines: Vec<String> = md.lines().map(String::from).collect();
    let marker = |text: &str, l: &String| l.trim().eq_ignore_ascii_case(text);
    if let Some(i) = lines.iter().position(|l| marker(OPEN, l)) {
        // A closing marker means an earlier run: replace all it wrote.
        let end = lines[i..]
            .iter()
            .position(|l| marker(CLOSE, l))
            .map_or(i, |n| i + n);
        lines.splice(i..=end, [block]);
    } else {
        let first = &headings[0];
        let title = has_title(&headings);
        // Under a title, or above the first section when there is no title.
        let at = if title {
            first.end_line + 1
        } else {
            first.line
        };
        let at = at.min(lines.len());
        lines.insert(
            at,
            if title {
                format!("\n{block}")
            } else {
                format!("{block}\n")
            },
        );
    }
    let n = list.len();
    let mut out = lines.join("\n");
    out.push('\n');
    Ok(OpResult::one(Produced::bytes(out.into_bytes(), Format::Md))
        .fact("entries", n)
        .note(format!("{n} entries")))
}
