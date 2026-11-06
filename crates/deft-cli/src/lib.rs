//! The `deft` command line: dispatch, help and rendering. Exposed as a library
//! so tests can call it without spawning a process.

pub mod args;
pub mod help;
pub mod render;
pub mod top;

use deft_core::error::{DeftError, ErrorCode, Result, err};
use deft_core::job::Ctx;
use deft_core::spec::Op;
use deft_ops::registry;
use deft_ops::runtime::{self, Request};

/// What a run produced. `main` prints it; tests inspect it.
#[derive(Debug, Default)]
pub struct Outcome {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub code: i32,
}

impl Outcome {
    pub fn text(s: impl Into<String>) -> Outcome {
        Outcome {
            stdout: s.into().into_bytes(),
            ..Default::default()
        }
    }
}

fn wants_json(args: &[String]) -> bool {
    args.iter().any(|a| a == "--json") || std::env::var_os("DEFT_JSON").is_some_and(|v| v != "0")
}

fn wants_help(args: &[String]) -> bool {
    let before_dash = args.iter().take_while(|a| *a != "--");
    before_dash.into_iter().any(|a| a == "--help" || a == "-h")
}

/// Run the CLI. `stdin` is called only if an input is `-`.
pub fn run(args: Vec<String>, stdin: impl FnOnce() -> Vec<u8>) -> Outcome {
    let json = wants_json(&args);
    match dispatch(&args, stdin) {
        Ok(o) => o,
        Err(e) => failure(&e, None, json),
    }
}

fn failure(e: &DeftError, op: Option<&str>, json: bool) -> Outcome {
    let body = if json {
        render::error_json_doc(op, e)
    } else {
        render::error_text(e)
    };
    Outcome {
        stdout: body.into_bytes(),
        stderr: vec![],
        code: e.code.exit_code(),
    }
}

fn dispatch(args: &[String], stdin: impl FnOnce() -> Vec<u8>) -> Result<Outcome> {
    let Some(first) = args.first().map(String::as_str) else {
        return Ok(Outcome::text(help::top()));
    };
    let rest = &args[1..];
    match first {
        "--help" | "-h" => Ok(Outcome::text(help::top())),
        "help" => help_topic(rest),
        "--version" | "-V" | "version" => Ok(Outcome::text(format!("deft {}\n", help::VERSION))),
        "find" => top::find(rest),
        "info" => top::info(rest, stdin),
        "convert" => top::convert(rest, stdin),
        "schema" => top::schema(rest),
        "engine" if rest.first().is_none_or(|s| s != "run") && wants_help(rest) => {
            Ok(Outcome::text(top::ENGINE_HELP.to_string()))
        }
        "engine" => top::engine(rest),
        "skill" if wants_help(rest) => Ok(Outcome::text(top::SKILL_HELP.to_string())),
        "skill" => top::skill(rest),
        "doctor" => top::doctor(),
        name => {
            let reg = registry();
            if let Some(group) = reg.group(name) {
                let Some(cmd) = rest
                    .first()
                    .map(String::as_str)
                    .filter(|c| !c.starts_with('-'))
                else {
                    return Ok(Outcome::text(help::group(group.name)));
                };
                let Some(found) = reg.resolve(group.name, cmd) else {
                    return Err(unknown(Some(group.name), cmd));
                };
                let tail = &rest[1..];
                if wants_help(tail) {
                    return Ok(Outcome::text(help::command(found.op, &found.path)));
                }
                return run_op(found.op, tail, stdin);
            }
            if let Some(plugin) = top::plugin_path(name) {
                return top::run_plugin(&plugin, rest);
            }
            Err(unknown(None, name))
        }
    }
}

fn unknown(group: Option<&str>, word: &str) -> DeftError {
    let reg = registry();
    let what = match group {
        Some(g) => format!("deft {g} has no command {word:?}"),
        None => format!("no group or command {word:?}"),
    };
    let e = err(ErrorCode::UnknownCommand, what);
    let suggestions = reg.suggest(group, word);
    if suggestions.is_empty() {
        e.hint(match group {
            Some(g) => {
                format!("deft {g} lists the commands; deft find \"<what you want>\" searches all")
            }
            None => "deft lists the groups; deft find \"<what you want>\" searches all".to_string(),
        })
    } else {
        e.hint(format!("did you mean: {}", suggestions.join(" | ")))
    }
}

fn help_topic(rest: &[String]) -> Result<Outcome> {
    let reg = registry();
    let words: Vec<&str> = rest
        .iter()
        .map(String::as_str)
        .filter(|a| !a.starts_with('-'))
        .collect();
    match words.as_slice() {
        [] => Ok(Outcome::text(help::top())),
        ["common"] => Ok(Outcome::text(help::common())),
        ["values"] => Ok(Outcome::text(help::values())),
        ["errors"] => Ok(Outcome::text(help::errors())),
        [g] => match reg.group(g) {
            Some(group) => Ok(Outcome::text(help::group(group.name))),
            None => Err(unknown(None, g)),
        },
        [g, c, ..] => {
            let group = reg.group(g).ok_or_else(|| unknown(None, g))?;
            let found = reg
                .resolve(group.name, c)
                .ok_or_else(|| unknown(Some(group.name), c))?;
            Ok(Outcome::text(help::command(found.op, &found.path)))
        }
    }
}

/// Everything needed to execute after parsing.
pub(crate) struct Session {
    pub ctx: Ctx,
    pub _tmp: tempfile::TempDir,
}

pub(crate) fn session(p: &args::Parsed) -> Result<Session> {
    let tmp = tempfile::tempdir()
        .map_err(|e| err(ErrorCode::OutputUnwritable, format!("temp directory: {e}")))?;
    let mut ctx = Ctx::new(tmp.path().to_path_buf());
    ctx.offline |= p.offline;
    ctx.install_engines |= p.install_engines;
    ctx.keep_metadata = p.keep_metadata;
    ctx.timeout = p.timeout.map(std::time::Duration::from_secs_f64);
    Ok(Session { ctx, _tmp: tmp })
}

pub(crate) fn request(p: args::Parsed, stdin: impl FnOnce() -> Vec<u8>) -> Request {
    let cores = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    let needs_stdin = p.inputs.iter().any(|i| i == "-");
    Request {
        stdin: needs_stdin.then(stdin),
        inputs: p.inputs,
        params: p.params,
        output: p.output,
        force: p.force,
        dry_run: p.dry_run,
        in_place: p.in_place,
        recursive: p.recursive,
        jobs: p.jobs.unwrap_or(cores),
        stdin_format: p.stdin_format,
    }
}

pub(crate) fn finish(report: runtime::Report, json: bool, quiet: bool) -> Outcome {
    let code = report.exit_code();
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let body = if json {
        render::json(&report)
    } else {
        render::text(&report)
    };
    match report.stdout {
        Some(bytes) => {
            // Binary payload owns stdout; the result line moves to stderr.
            stdout = bytes;
            if !quiet {
                stderr = body.into_bytes();
            }
        }
        None if !quiet || code != 0 => stdout = body.into_bytes(),
        None => {}
    }
    Outcome {
        stdout,
        stderr,
        code,
    }
}

fn run_op(op: &'static Op, args: &[String], stdin: impl FnOnce() -> Vec<u8>) -> Result<Outcome> {
    let json = wants_json(args);
    let parsed = args::parse(args, op.params).map_err(|e| add_usage_hint(e, op))?;
    let session = session(&parsed)?;
    let (json_out, quiet) = (parsed.json || json, parsed.quiet);
    let req = request(parsed, stdin);
    match runtime::execute(op, req, &session.ctx) {
        Ok(report) => Ok(finish(report, json_out, quiet)),
        Err(e) => Ok(failure(&e, Some(op.id), json_out)),
    }
}

fn add_usage_hint(e: DeftError, op: &Op) -> DeftError {
    if e.hint.is_none() && e.code == ErrorCode::BadValue && e.message.starts_with("--to:") {
        return e.hint("for other formats: deft convert <input> -o <name.ext>");
    }
    if e.hint.is_none() && matches!(e.code, ErrorCode::BadOption | ErrorCode::BadValue) {
        e.hint(format!("deft {} --help", op.path()))
    } else {
        e
    }
}
