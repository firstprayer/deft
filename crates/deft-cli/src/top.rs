//! Top-level commands: find, info, convert, schema, engine, skill, doctor, plugins.

use crate::{Outcome, args, finish, help, request, session};
use deft_core::error::{ErrorCode, Result, err};
use deft_core::format::sniff_file;
use deft_core::spec::{Accept, InputArity, Op};
use deft_core::value::{Kind, format_bytes};
use deft_ops::registry;
use deft_ops::runtime::{self, Item, Report};
use serde_json::json;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

pub fn find(rest: &[String]) -> Result<Outcome> {
    let query = rest
        .iter()
        .filter(|a| !a.starts_with("--"))
        .cloned()
        .collect::<Vec<_>>()
        .join(" ");
    if query.trim().is_empty() {
        return Err(
            err(ErrorCode::BadOption, "deft find needs words").hint("deft find \"shrink pdf\"")
        );
    }
    let hits = registry().find(&query);
    let unknown = registry().unknown_job(&query);
    if rest.iter().any(|a| a == "--json") {
        let list: Vec<_> =
            hits.iter().map(|(p, op, _)| json!({"command": format!("deft {p}"), "op": op.id, "summary": op.summary, "signature": op.signature()})).collect();
        return Ok(Outcome::text(format!(
            "{}\n",
            if list.is_empty() {
                json!({"v": 1, "ok": true, "matches": list, "unknown_words": unknown, "note": "no command does this"})
            } else {
                json!({"v": 1, "ok": true, "matches": list, "unknown_words": unknown})
            }
        )));
    }
    if hits.is_empty() {
        let why = if unknown.is_empty() {
            String::new()
        } else {
            format!(" (nothing knows {:?})", unknown.join(" "))
        };
        return Ok(Outcome::text(format!(
            "no command does this{why}; use another tool. All groups: deft --help\n"
        )));
    }
    let w = hits.iter().map(|h| h.0.len()).max().unwrap_or(0);
    let sw = hits
        .iter()
        .map(|h| h.1.signature().len())
        .max()
        .unwrap_or(0);
    let mut s = String::new();
    for (path, op, _) in &hits {
        let _ = writeln!(s, "{path:<w$}  {:<sw$}  {}", op.signature(), op.summary);
    }
    Ok(Outcome::text(s))
}

static INFO: Op = Op {
    id: "info",
    summary: "Identify any file",
    ..Op::DEFAULT
};

pub fn info(rest: &[String], stdin: impl FnOnce() -> Vec<u8>) -> Result<Outcome> {
    let parsed = args::parse(rest, &[])?;
    if parsed.help {
        return Ok(Outcome::text(
            "deft info <file>... - Type, size, pages, dimensions, duration\nAdd --json for all fields. Video and audio details need ffprobe (ships with ffmpeg).\n",
        ));
    }
    if parsed.inputs.is_empty() {
        return Err(
            err(ErrorCode::BadOption, "deft info needs a file").hint("deft info report.pdf")
        );
    }
    if parsed.output.is_some() {
        return Err(err(
            ErrorCode::BadOption,
            "deft info prints its result; it has no -o",
        )
        .hint("deft info <file> --json > info.json"));
    }
    let (json, quiet) = (parsed.json, parsed.quiet);
    let s = session(&parsed)?;
    let started = std::time::Instant::now();
    let _ = stdin;
    let mut items = Vec::new();
    for raw in &parsed.inputs {
        let path = Path::new(raw);
        if !path.exists() {
            items.push(Item {
                inputs: vec![raw.clone()],
                error: Some(err(
                    ErrorCode::InputNotFound,
                    format!("{raw}: no such file or directory"),
                )),
                ..Default::default()
            });
            continue;
        }
        if path.is_dir() {
            items.push(Item {
                inputs: vec![raw.clone()],
                note: "directory".into(),
                facts: [("format".to_string(), json!("directory"))]
                    .into_iter()
                    .collect(),
                ..Default::default()
            });
            continue;
        }
        let bytes = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        let format = sniff_file(path).ok().flatten();
        let dedicated = format.and_then(|f| {
            registry()
                .ops
                .iter()
                .copied()
                .find(|op| op.command() == "info" && op.input.accepts(f))
        });
        let mut item = match dedicated {
            Some(op) => {
                let req = runtime::Request {
                    inputs: vec![raw.clone()],
                    jobs: 1,
                    ..Default::default()
                };
                match runtime::execute(op, req, &s.ctx) {
                    Ok(mut r) => r.items.remove(0),
                    Err(e) => Item {
                        inputs: vec![raw.clone()],
                        error: Some(e),
                        ..Default::default()
                    },
                }
            }
            None => Item {
                inputs: vec![raw.clone()],
                ..Default::default()
            },
        };
        // A locked file is still identified; only its contents are out of reach.
        if item
            .error
            .as_ref()
            .is_some_and(|e| e.code == ErrorCode::InputEncrypted)
        {
            item.error = None;
            item.facts.insert("encrypted".into(), json!(true));
            item.note =
                format!("password-protected; for more: deft pdf info {raw} --password <text>");
        }
        // JSON that is not a list of rows (a package.json, an API reply) is
        // still identified, with its shape.
        if format == Some(deft_core::format::Format::Json)
            && item
                .error
                .as_ref()
                .is_some_and(|e| e.code == ErrorCode::InputUnsupported)
            && let Some(shape) = std::fs::read(path)
                .ok()
                .and_then(|b| deft_tools_file::media::json_shape(&b))
        {
            item.error = None;
            item.note = match (shape.get("keys"), shape.get("items")) {
                (Some(k), _) => format!("object with {k} keys"),
                (_, Some(n)) => format!("array of {n} items"),
                _ => String::new(),
            };
            item.facts.extend(shape);
        }
        if item.error.is_none() && format.is_some_and(|f| f.is_video() || f.is_audio()) {
            media_info(&s.ctx, path, &mut item);
        }
        if item.error.is_none() {
            let label = format.map(|f| f.label()).unwrap_or("unknown");
            item.facts.insert("format".into(), json!(label));
            item.facts.insert("bytes".into(), json!(bytes));
            let head = format!("{label}, {}", format_bytes(bytes));
            item.note = if item.note.is_empty() {
                head
            } else {
                format!("{head}, {}", item.note)
            };
        }
        items.push(item);
    }
    let report = Report {
        op: &INFO,
        items,
        elapsed_ms: started.elapsed().as_millis(),
        dry_run: false,
        stdout: None,
        chain: vec![],
    };
    Ok(finish(report, json, quiet))
}

/// Duration, picture size and codecs of a video or audio file, from ffprobe.
/// Without ffprobe the file is still identified, and the warning says what is
/// missing and why.
fn media_info(ctx: &deft_core::job::Ctx, path: &Path, item: &mut Item) {
    let Some(ffprobe) = deft_engines::locate(ctx, "ffprobe") else {
        item.warnings.push(
            "duration and picture size need ffprobe, which is not installed (see deft doctor)"
                .into(),
        );
        return;
    };
    let args = [
        "-v",
        "error",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
    ]
    .iter()
    .map(|s| s.to_string())
    .chain([path.display().to_string()])
    .collect::<Vec<_>>();
    let probe = deft_engines::run(ctx, &ffprobe, &args)
        .ok()
        .and_then(|out| serde_json::from_slice::<serde_json::Value>(&out.stdout).ok());
    let Some(probe) = probe else {
        item.warnings
            .push("ffprobe could not read the file: no duration or picture size".into());
        return;
    };
    let facts = deft_tools_file::media::facts(&probe);
    item.note = deft_tools_file::media::summary(&facts);
    item.facts.extend(facts);
}

/// The `--to` option of `deft convert`.
pub(crate) const CONVERT_PARAMS: &[deft_core::spec::ParamSpec] =
    &[deft_core::spec::param("to", Kind::Text, "Target format")];

pub fn convert(rest: &[String], stdin: impl FnOnce() -> Vec<u8>) -> Result<Outcome> {
    // Options of the converters a chain may run, such as --images.
    let mut specs = CONVERT_PARAMS.to_vec();
    for op in registry().ops.iter().filter(|op| !op.converts.is_empty()) {
        for p in op.params {
            if !specs.iter().any(|s| s.name == p.name) {
                specs.push(*p);
            }
        }
    }
    let parsed = args::parse(rest, &specs)?;
    if parsed.help {
        let mut s = String::from(
            "deft convert <in>... [-o <file|dir>] [--to <format>] - Any-to-any conversion\n",
        );
        s.push_str("The target comes from --to or the -o extension; deft picks the best chain of converters.\n");
        s.push_str("--dry-run shows the chain. Several inputs need -o <dir>/ or --to.\n");
        s.push_str(
            "Options of a step pass through, e.g. --images <dir> when the target is Markdown.\n",
        );
        s.push_str("\nExamples:\n  deft convert photo.png -o photo.pdf\n  deft convert notes.md --to html\n  deft convert *.png --to webp -o web/\n");
        return Ok(Outcome::text(s));
    }
    let to = parsed.params.opt_text("to");
    let (json, quiet) = (parsed.json, parsed.quiet);
    let s = session(&parsed)?;
    let req = request(parsed, stdin);
    let report = runtime::convert(req, to, &s.ctx)?;
    Ok(finish(report, json, quiet))
}

pub fn schema(rest: &[String]) -> Result<Outcome> {
    let words: Vec<&str> = rest
        .iter()
        .map(String::as_str)
        .filter(|a| !a.starts_with('-'))
        .collect();
    let [g, c] = words.as_slice() else {
        return Err(err(
            ErrorCode::BadOption,
            "usage: deft schema <group> <command>",
        ));
    };
    let found = registry().resolve(g, c).ok_or_else(|| {
        err(
            ErrorCode::UnknownCommand,
            format!("no command deft {g} {c}"),
        )
    })?;
    Ok(Outcome::text(format!(
        "{}\n",
        serde_json::to_string_pretty(&op_schema(found.op)).unwrap_or_default()
    )))
}

/// JSON Schema for an operation's inputs and options.
pub fn op_schema(op: &Op) -> serde_json::Value {
    let mut props = serde_json::Map::new();
    let mut required = Vec::new();
    for p in op.params {
        let mut s = p.kind.schema();
        s["description"] = json!(p.help);
        if let Some(d) = p.default {
            s["default"] = json!(d);
        }
        props.insert(p.name.to_string(), s);
        if p.required {
            required.push(p.name);
        }
    }
    let accept = match op.input.accept {
        Accept::Formats(f) => json!(f.iter().map(|x| x.ext()).collect::<Vec<_>>()),
        Accept::AnyFile => json!("any file"),
        Accept::Url => json!("url"),
        Accept::Nothing => json!("none"),
    };
    let multiple = matches!(op.input.arity, InputArity::All { .. } | InputArity::Each);
    json!({
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": op.id,
        "description": op.summary,
        "type": "object",
        "properties": props,
        "required": required,
        "x-deft": {"accept": accept, "multiple_inputs": multiple, "output": op.output.label, "engines": op.engines, "paths": op.paths()},
    })
}

fn engine_ctx() -> Result<(deft_core::job::Ctx, tempfile::TempDir)> {
    let tmp = tempfile::tempdir().map_err(|e| err(ErrorCode::OutputUnwritable, e.to_string()))?;
    Ok((deft_core::job::Ctx::new(tmp.path().to_path_buf()), tmp))
}

pub const ENGINE_HELP: &str = "deft engine <command> - Manage the programs deft runs

Commands:
  list                    Show each engine and where it is
  path <name>             Print where an engine is
  run <name> -- <args>    Run an engine directly

Example:
  deft engine list
";

pub fn engine(rest: &[String]) -> Result<Outcome> {
    let (ctx, _tmp) = engine_ctx()?;
    let sub = rest.first().map(String::as_str).unwrap_or("list");
    let name = rest.get(1).map(String::as_str);
    match sub {
        "list" => {
            let mut s = String::new();
            for e in deft_engines::ENGINES {
                match deft_engines::locate(&ctx, e.name) {
                    Some(p) => {
                        let _ = writeln!(s, "{:<8} ok       {}", e.name, p.display());
                    }
                    None => {
                        let _ = writeln!(s, "{:<8} missing  {}", e.name, e.manual);
                    }
                }
            }
            Ok(Outcome::text(s))
        }
        "path" => {
            let n =
                name.ok_or_else(|| err(ErrorCode::BadOption, "usage: deft engine path <name>"))?;
            match deft_engines::locate(&ctx, n) {
                Some(p) => Ok(Outcome::text(format!("{}\n", p.display()))),
                None => Err(err(ErrorCode::EngineMissing, format!("{n} not found"))),
            }
        }
        "run" => {
            let n = name.ok_or_else(|| {
                err(
                    ErrorCode::BadOption,
                    "usage: deft engine run <name> -- <args>",
                )
            })?;
            let path = deft_engines::require(&ctx, n, "deft engine run")?;
            let args: Vec<String> = rest[2..]
                .iter()
                .skip_while(|a| *a == "--")
                .cloned()
                .collect();
            let out = std::process::Command::new(path)
                .args(args)
                .output()
                .map_err(|e| err(ErrorCode::EngineFailed, e.to_string()))?;
            Ok(Outcome {
                stdout: out.stdout,
                stderr: out.stderr,
                code: out.status.code().unwrap_or(1),
            })
        }
        other => Err(err(
            ErrorCode::UnknownCommand,
            format!("no engine command {other:?}"),
        )
        .hint("deft engine list | path | run")),
    }
}

/// The SKILL.md text, generated from the registry so it never goes stale.
pub fn skill_text() -> String {
    let reg = registry();
    let mut s = String::from(
        "---\nname: deft\ndescription: \"Use deft first for any simple, direct operation on local files, whatever the type: create, convert, read, query, extract, edit, compress, merge, split, compare or inspect. Try it before writing a script or one-liner. If unsure, run deft find; if it says no command does this, use another tool. Not for programming work: to read, write or change source code, skip it.\"\n---\n\n# deft\n\nOne local command for file jobs. Prefer it over writing Python or shell scripts.\n\n",
    );
    s.push_str("Syntax: `deft <group> <command> [inputs] [-o output] [--options]`\n\nGroups:\n");
    for g in &reg.groups {
        let _ = writeln!(s, "- `{}`: {}", g.name, g.summary);
    }
    s.push_str("\nHow to use it:\n");
    s.push_str(
        "1. Run `deft <group>` to list commands, or `deft find \"<what you want>\"` to search. If it says `no command does this`, use another tool.\n",
    );
    s.push_str("2. Run `deft <group> <command> --help` for options and examples.\n");
    s.push_str("3. `deft convert <in> -o <out.ext>` converts between formats by extension.\n");
    s.push_str("4. Results are one line starting with `ok`, `partial` or `error`; add `--json` to parse them. Errors include a `hint:` with the fix.\n");
    s.push_str(
        "\nInputs are never changed or overwritten. `--force` lets an output replace an existing file that is not an input. Globs and directories work as inputs; several inputs run in parallel.\n\nIf `deft` is not installed, ask the user, then follow https://github.com/firstprayer/deft/blob/main/docs/INSTALL.md\n",
    );
    s
}

pub const SKILL_HELP: &str = "deft skill <command> - Teach an agent to use deft

Commands:
  print                       Print the skill text
  install                     Write it to the agent's skills folder

Options for install:
  --agent claude|codex|all    Which agent; default all
  --scope user|project        Home folder or this folder; default user
  --force                     Replace a SKILL.md that differs (an edited copy)

Example:
  deft skill install --agent claude
";

pub fn skill(rest: &[String]) -> Result<Outcome> {
    let sub = rest.first().map(String::as_str).unwrap_or("print");
    match sub {
        "print" => Ok(Outcome::text(skill_text())),
        "install" => {
            let opt = |name: &str| {
                rest.iter()
                    .position(|a| a == name)
                    .and_then(|i| rest.get(i + 1))
                    .map(String::as_str)
            };
            let agent = opt("--agent").unwrap_or("all");
            let scope = opt("--scope").unwrap_or("user");
            let agents: &[&str] = match agent {
                "claude" => &["claude"],
                "codex" => &["codex"],
                "all" => &["claude", "codex"],
                other => {
                    return Err(err(
                        ErrorCode::BadValue,
                        format!("--agent: {other:?} is not one of claude, codex, all"),
                    ));
                }
            };
            let base = match scope {
                "user" => PathBuf::from(
                    std::env::var_os("HOME")
                        .or_else(|| std::env::var_os("USERPROFILE"))
                        .ok_or_else(|| err(ErrorCode::OutputUnwritable, "no home directory"))?,
                ),
                "project" => PathBuf::from("."),
                other => {
                    return Err(err(
                        ErrorCode::BadValue,
                        format!("--scope: {other:?} is not one of user, project"),
                    ));
                }
            };
            let force = rest.iter().any(|a| a == "--force");
            let paths: Vec<(&str, PathBuf)> = agents
                .iter()
                .map(|a| {
                    // Codex reads skills from the shared `.agents` folder.
                    let folder = if *a == "codex" { "agents" } else { a };
                    (*a, base.join(format!(".{folder}/skills/deft/SKILL.md")))
                })
                .collect();
            let s = install_skill(&paths, force)?;
            Ok(Outcome::text(s))
        }
        other => Err(err(
            ErrorCode::UnknownCommand,
            format!("no skill command {other:?}"),
        )
        .hint("deft skill print | install --agent claude|codex|all --scope user|project")),
    }
}

/// Write the skill to each path. A SKILL.md that differs may hold the user's
/// own edits: it is replaced only with `force`, and nothing is written until
/// every path is clear.
fn install_skill(paths: &[(&str, PathBuf)], force: bool) -> Result<String> {
    let text = skill_text();
    if !force
        && let Some((_, path)) = paths
            .iter()
            .find(|(_, p)| std::fs::read_to_string(p).is_ok_and(|old| old != text))
    {
        return Err(err(
            ErrorCode::OutputExists,
            format!(
                "{} differs from this deft's skill (edited, or from another version)",
                path.display()
            ),
        )
        .hint("add --force to replace it, or deft skill print to compare"));
    }
    let mut s = String::new();
    for (a, path) in paths {
        std::fs::create_dir_all(path.parent().expect("has parent"))
            .map_err(|e| err(ErrorCode::OutputUnwritable, e.to_string()))?;
        std::fs::write(path, &text).map_err(|e| err(ErrorCode::OutputUnwritable, e.to_string()))?;
        let _ = writeln!(s, "ok skill.install {a} -> {}", path.display());
    }
    Ok(s)
}

pub fn doctor(rest: &[String]) -> Result<Outcome> {
    let (ctx, _tmp) = engine_ctx()?;
    let reg = registry();
    let engines: Vec<_> = deft_engines::ENGINES
        .iter()
        .map(|e| {
            let path = deft_engines::locate(&ctx, e.name);
            let version = path
                .as_ref()
                .and_then(|p| deft_engines::version(p, e.version_arg));
            (e, path, version)
        })
        .collect();
    if rest.iter().any(|a| a == "--json") {
        let list: Vec<_> = engines
            .iter()
            .map(|(e, path, version)| {
                json!({
                    "name": e.name,
                    "found": path.is_some(),
                    "path": path.as_ref().map(|p| p.display().to_string()),
                    "version": version,
                    "purpose": e.purpose,
                    "install": e.manual,
                })
            })
            .collect();
        let report = json!({
            "v": 1,
            "ok": true,
            "version": help::VERSION,
            "groups": reg.groups.len(),
            "operations": reg.ops.len(),
            "engine_dir": ctx.engine_dir.display().to_string(),
            "engines": list,
        });
        return Ok(Outcome::text(format!("{report}\n")));
    }
    let mut s = format!(
        "deft {}\n{} groups, {} operations\n",
        help::VERSION,
        reg.groups.len(),
        reg.ops.len()
    );
    let _ = writeln!(s, "engine dir: {}", ctx.engine_dir.display());
    for (e, path, version) in &engines {
        match path {
            Some(_) => {
                let v = version.as_deref().unwrap_or("version unknown");
                let _ = writeln!(s, "ok      {:<8} {v}", e.name);
            }
            None => {
                let _ = writeln!(s, "missing {:<8} {}: {}", e.name, e.purpose, e.manual);
            }
        }
    }
    Ok(Outcome::text(s))
}

/// External `deft-<name>` executables on PATH.
pub fn plugins() -> Vec<String> {
    let Some(path) = std::env::var_os("PATH") else {
        return vec![];
    };
    let mut found = Vec::new();
    for dir in std::env::split_paths(&path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if let Some(n) = plugin_name(&name, cfg!(windows))
                && !found.iter().any(|f| f == n)
            {
                found.push(n.to_string());
            }
        }
    }
    found.sort();
    found
}

/// The group a file on the PATH adds. Only programs count, so a Windows `.pdb`
/// or `.d` file beside one does not, and neither does Cargo's `deft-<hash>` build
/// output, which is on the PATH while tests run on Windows.
fn plugin_name(file: &str, windows: bool) -> Option<&str> {
    let name = file.strip_prefix("deft-")?;
    let name = if windows {
        name.strip_suffix(".exe")?
    } else {
        name
    };
    let hash = name.len() == 16 && name.bytes().all(|b| b.is_ascii_hexdigit());
    (!name.is_empty() && !name.contains('.') && !hash).then_some(name)
}

pub fn plugin_path(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let file = if cfg!(windows) {
        format!("deft-{name}.exe")
    } else {
        format!("deft-{name}")
    };
    std::env::split_paths(&path)
        .map(|d| d.join(&file))
        .find(|p| p.is_file())
}

pub fn run_plugin(path: &Path, args: &[String]) -> Result<Outcome> {
    let out = std::process::Command::new(path)
        .args(args)
        .output()
        .map_err(|e| err(ErrorCode::EngineFailed, format!("{}: {e}", path.display())))?;
    Ok(Outcome {
        stdout: out.stdout,
        stderr: out.stderr,
        code: out.status.code().unwrap_or(1),
    })
}

#[cfg(test)]
mod tests {
    use super::{install_skill, plugin_name, skill_text};
    use std::path::PathBuf;

    #[test]
    fn an_edited_skill_is_kept_unless_forced() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".claude/skills/deft/SKILL.md");
        let paths: Vec<(&str, PathBuf)> = vec![("claude", path.clone())];
        install_skill(&paths, false).unwrap();
        // The same text again is fine.
        install_skill(&paths, false).unwrap();
        std::fs::write(&path, "my own notes").unwrap();
        let e = install_skill(&paths, false).unwrap_err();
        assert!(e.message.contains("differs"), "{}", e.message);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "my own notes");
        install_skill(&paths, true).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), skill_text());
    }

    #[test]
    fn only_programs_are_plugins() {
        assert_eq!(plugin_name("deft-audio2", false), Some("audio2"));
        assert_eq!(plugin_name("deft-audio2.exe", true), Some("audio2"));
        assert_eq!(plugin_name("deft-audio2.pdb", true), None);
        assert_eq!(plugin_name("deft-audio2.d", false), None);
        assert_eq!(plugin_name("deft-audio2", true), None);
        assert_eq!(plugin_name("deft-1312041eab5a1b86.exe", true), None);
        assert_eq!(plugin_name("deft-", false), None);
        assert_eq!(plugin_name("other", false), None);
    }
}
