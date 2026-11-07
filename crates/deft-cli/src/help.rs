//! The three help layers (top, group, command) plus reference topics. Every
//! layer has a token budget enforced by the contract tests.

use crate::args::COMMON;
use deft_core::error::ErrorCode;
use deft_core::spec::{InputArity, Op, OutputArity, Stability};
use deft_core::value::{Kind, syntax_help};
use deft_ops::registry;
use std::fmt::Write as _;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

fn engine_note(op: &Op) -> String {
    let mut flags = Vec::new();
    if op.stability == Stability::Beta {
        flags.push("beta".to_string());
    }
    if flags.is_empty() {
        String::new()
    } else {
        format!(" [{}]", flags.join(", "))
    }
}

/// A command outside any group. Help and the developer catalog read this list.
pub struct TopCommand {
    pub name: &'static str,
    pub usage: &'static str,
    pub summary: &'static str,
    pub example: &'static str,
    /// Shown under "Any file" in `deft`; the others are named on one line.
    pub any_file: bool,
}

pub const TOP_COMMANDS: &[TopCommand] = &[
    TopCommand {
        name: "convert",
        usage: "<in>... -o <out>",
        summary: "Any-to-any by extension, e.g. deft convert a.png -o a.pdf",
        example: "deft convert photo-1.jpg -o photo.pdf",
        any_file: true,
    },
    TopCommand {
        name: "info",
        usage: "<file>...",
        summary: "Type, size, pages, dimensions, duration",
        example: "deft info report.pdf",
        any_file: true,
    },
    TopCommand {
        name: "find",
        usage: "<words>",
        summary: "Search commands by intent",
        example: "deft find \"shrink pdf\"",
        any_file: true,
    },
    TopCommand {
        name: "schema",
        usage: "<group> <command>",
        summary: "JSON Schema of a command's inputs and options",
        example: "deft schema pdf merge",
        any_file: false,
    },
    TopCommand {
        name: "engine",
        usage: "list|path|run",
        summary: "Find the external programs deft runs, such as ffmpeg",
        example: "deft engine list",
        any_file: false,
    },
    TopCommand {
        name: "skill",
        usage: "print|install",
        summary: "Print or install the agent skill",
        example: "deft skill print",
        any_file: false,
    },
    TopCommand {
        name: "doctor",
        usage: "",
        summary: "Check the environment and engines",
        example: "deft doctor",
        any_file: false,
    },
];

pub fn top() -> String {
    let reg = registry();
    let mut s = format!(
        "deft {VERSION} - local file toolkit. Usage: deft <group> <command> [inputs] [-o output]\n\nGroups:\n"
    );
    for g in &reg.groups {
        let _ = writeln!(s, "  {:<8} {}", g.name, g.summary);
    }
    s.push_str("\nAny file:\n");
    for c in TOP_COMMANDS.iter().filter(|c| c.any_file) {
        let _ = writeln!(s, "  {:<8} {:<18} {}", c.name, c.usage, c.summary);
    }
    let plugins = crate::top::plugins();
    if !plugins.is_empty() {
        let _ = writeln!(s, "\nPlugins: {}", plugins.join(", "));
    }
    s.push_str("\nNext: deft <group> | deft <group> <command> --help | deft help common\n");
    let other: Vec<&str> = TOP_COMMANDS
        .iter()
        .filter(|c| !c.any_file)
        .map(|c| c.name)
        .collect();
    let _ = writeln!(
        s,
        "Other: {}. Results are one line; add --json for structure.",
        other.join(", ")
    );
    s
}

pub fn group(name: &str) -> String {
    let reg = registry();
    let mut s = format!("deft {name} - commands (input -> output)\n");
    let commands = reg.commands(name);
    let w = commands.iter().map(|(c, _)| c.len()).max().unwrap_or(0);
    let sw = commands
        .iter()
        .map(|(_, o)| o.signature().len())
        .max()
        .unwrap_or(0);
    for (cmd, op) in &commands {
        let _ = writeln!(
            s,
            "  {cmd:<w$}  {:<sw$}  {}{}",
            op.signature(),
            op.summary,
            engine_note(op)
        );
    }
    let _ = writeln!(s, "More: deft {name} <command> --help");
    s
}

fn placeholder(kind: &Kind) -> String {
    let p = kind.placeholder();
    if p.is_empty() {
        String::new()
    } else {
        format!(" {p}")
    }
}

pub fn command(op: &Op, path: &str) -> String {
    let mut s = format!("deft {path} - {}{}\n", op.summary, engine_note(op));
    let others: Vec<String> = op
        .paths()
        .into_iter()
        .filter(|p| p != path)
        .map(|p| format!("deft {p}"))
        .collect();
    let converts = !op.converts.is_empty();
    if !others.is_empty() || converts {
        let mut also = others;
        if converts {
            also.push("deft convert".to_string());
        }
        let _ = writeln!(s, "Same as: {}", also.join(" · "));
    }
    let input = match op.input.arity {
        InputArity::None => String::new(),
        InputArity::Each => format!("<{}>...", op.input.label),
        InputArity::All { .. } => format!("<{}>...", op.input.label),
    };
    let out_hint = match op.output.arity {
        OutputArity::Report => String::new(),
        OutputArity::Many | OutputArity::Dir => " [-o <dir>]".into(),
        OutputArity::One => format!(" [-o <{}|dir>]", op.output.label),
    };
    let _ = writeln!(s, "\nUsage: deft {path} {input}{out_hint} [options]");
    let each = match op.input.arity {
        InputArity::Each => " (each handled separately)",
        InputArity::All { .. } => " (all together, in order)",
        InputArity::None => "",
    };
    let default_out = match (op.output.arity, op.suffix) {
        (OutputArity::Report, _) => "printed result".to_string(),
        (OutputArity::Many, Some(sfx)) => format!("{} files in <name>-{sfx}/", op.output.label),
        (OutputArity::Dir, _) => "directory <name>/".to_string(),
        (_, Some(sfx)) => format!("{} (default <name>.{sfx}.<ext>)", op.output.label),
        (_, None) => format!("{} (default <name>.<ext>)", op.output.label),
    };
    let _ = writeln!(s, "In: {}{each}   Out: {default_out}", op.input.label);
    for d in op.details {
        let _ = writeln!(s, "{d}");
    }
    if !op.params.is_empty() {
        s.push_str("\nOptions:\n");
        for p in op.params {
            let mut line = format!("  --{}{}", p.name, placeholder(&p.kind));
            let mut tail = String::from(p.help);
            if p.required {
                tail.push_str(" (required)");
            } else if let Some(d) = p.default {
                let _ = write!(tail, " (default {d})");
            }
            if line.len() < 30 {
                let _ = writeln!(s, "{line:<30} {tail}");
            } else {
                line.push('\n');
                s.push_str(&line);
                let _ = writeln!(s, "      {tail}");
            }
        }
    }
    s.push_str("Common: -o --force --dry-run --json -j   (deft help common)\n");
    if !op.examples.is_empty() {
        s.push_str("\nExamples:\n");
        for e in op.examples {
            let _ = writeln!(s, "  {e}");
        }
    }
    if !op.result.is_empty() {
        let _ = writeln!(s, "\nResult fields: {}", op.result.join(", "));
    }
    if !op.engines.is_empty() {
        let _ = writeln!(s, "Needs: {}", op.engines.join(", "));
    }
    if !op.errors.is_empty() {
        let codes: Vec<&str> = op.errors.iter().map(|e| e.as_str()).collect();
        let _ = writeln!(s, "Errors: {}", codes.join(", "));
    }
    s
}

pub fn common() -> String {
    let mut s = String::from("Options available on every command:\n");
    for (name, short, value, help) in COMMON {
        let flag = match short {
            Some(c) => format!("-{c}, --{name}"),
            None => format!("--{name}"),
        };
        let flag = if *value {
            format!("{flag} <value>")
        } else {
            flag
        };
        let _ = writeln!(s, "  {flag:<28} {help}");
    }
    s.push_str("\nInputs: files, globs (expanded by deft), directories, - for stdin.\n");
    s.push_str("Outputs never overwrite inputs or existing files unless asked.\n");
    s.push_str("Value syntax: deft help values. Error codes: deft help errors.\n");
    s
}

pub fn values() -> String {
    let mut s = String::from("Value syntax shared by all commands:\n");
    for (name, kind) in [
        ("pages", Kind::Pages),
        ("time", Kind::Duration),
        ("times", Kind::Times),
        ("size", Kind::Size),
        ("length", Kind::Length),
        ("bytes", Kind::Bytes),
        ("color", Kind::Color),
        ("position", Kind::Position),
    ] {
        let _ = writeln!(s, "  <{name}>  {}", syntax_help(&kind));
    }
    s
}

pub fn errors() -> String {
    let mut s = String::from(
        "Error codes (stable). Exit: 0 ok, 1 failed, 2 usage, 3 input, 4 engine missing, 5 partial.\n",
    );
    for c in ErrorCode::ALL {
        let _ = writeln!(s, "  {:<20} {} {}", c.as_str(), c.exit_code(), c.doc());
    }
    s
}
