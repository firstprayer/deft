//! Argument parsing. Option tables come from the operation specs, so only the
//! requested command is ever parsed and startup does not grow with the catalog.

use deft_core::error::{ErrorCode, Result, err};
use deft_core::format::Format;
use deft_core::job::Params;
use deft_core::spec::ParamSpec;
use deft_core::value::{Kind, Value, parse_duration};
use deft_ops::registry::distance;

/// Options available on every command.
pub const COMMON: &[(&str, Option<char>, bool, &str)] = &[
    (
        "output",
        Some('o'),
        true,
        "Output file or directory; - writes to stdout",
    ),
    ("force", None, false, "Overwrite an existing output"),
    (
        "dry-run",
        None,
        false,
        "Show what would happen; write nothing",
    ),
    (
        "json",
        None,
        false,
        "Structured result (or set DEFT_JSON=1)",
    ),
    ("quiet", Some('q'), false, "No output on success"),
    (
        "jobs",
        Some('j'),
        true,
        "Parallel jobs (default: CPU cores)",
    ),
    (
        "recursive",
        Some('r'),
        false,
        "Descend into directories given as input",
    ),
    (
        "in-place",
        None,
        false,
        "Replace the input file (atomically)",
    ),
    (
        "stdin-format",
        None,
        true,
        "Format of stdin when it cannot be detected",
    ),
    (
        "timeout",
        None,
        true,
        "Time limit for engine calls, e.g. 5m",
    ),
    (
        "keep-metadata",
        None,
        false,
        "Keep original metadata where supported",
    ),
    ("help", Some('h'), false, "Show help"),
];

#[derive(Debug, Default)]
pub struct Parsed {
    pub inputs: Vec<String>,
    pub params: Params,
    pub output: Option<String>,
    pub force: bool,
    pub dry_run: bool,
    pub json: bool,
    pub quiet: bool,
    pub jobs: Option<usize>,
    pub recursive: bool,
    pub in_place: bool,
    pub stdin_format: Option<Format>,
    pub timeout: Option<f64>,
    pub keep_metadata: bool,
    pub help: bool,
}

fn set_common(p: &mut Parsed, name: &str, value: Option<String>) -> Result<()> {
    let need = |v: Option<String>| {
        v.ok_or_else(|| err(ErrorCode::BadOption, format!("--{name} needs a value")))
    };
    match name {
        "output" => p.output = Some(need(value)?),
        "force" => p.force = true,
        "dry-run" => p.dry_run = true,
        "json" => p.json = true,
        "quiet" => p.quiet = true,
        "jobs" => {
            let raw = need(value)?;
            p.jobs = Some(raw.parse().ok().filter(|n| *n > 0).ok_or_else(|| {
                err(
                    ErrorCode::BadValue,
                    format!("--jobs: {raw:?} is not a positive number"),
                )
            })?);
        }
        "recursive" => p.recursive = true,
        "in-place" => p.in_place = true,
        "stdin-format" => {
            let raw = need(value)?;
            p.stdin_format = Some(Format::from_name(&raw).ok_or_else(|| {
                err(
                    ErrorCode::BadValue,
                    format!("--stdin-format: unknown format {raw:?}"),
                )
            })?);
        }
        "timeout" => {
            p.timeout = Some(
                parse_duration(&need(value)?)
                    .map_err(|m| err(ErrorCode::BadValue, format!("--timeout: {m}")))?,
            )
        }
        "keep-metadata" => p.keep_metadata = true,
        "help" => p.help = true,
        _ => unreachable!("not a common option: {name}"),
    }
    Ok(())
}

fn unknown_option(name: &str, params: &[ParamSpec]) -> deft_core::error::DeftError {
    let mut names: Vec<&str> = params.iter().map(|p| p.name).collect();
    names.extend(COMMON.iter().map(|c| c.0));
    let best = names.iter().map(|n| (distance(name, n), *n)).min();
    let e = err(ErrorCode::BadOption, format!("unknown option --{name}"));
    match best {
        Some((d, n)) if d <= 3 => e.hint(format!("did you mean --{n}?")),
        _ => e,
    }
}

/// Parse everything after the command path.
pub fn parse(args: &[String], params: &[ParamSpec]) -> Result<Parsed> {
    let mut p = Parsed::default();
    let mut i = 0;
    let mut only_positional = false;
    while i < args.len() {
        let arg = &args[i];
        i += 1;
        if only_positional || arg == "-" || !arg.starts_with('-') {
            p.inputs.push(arg.clone());
            continue;
        }
        if arg == "--" {
            only_positional = true;
            continue;
        }
        if let Some(body) = arg.strip_prefix("--") {
            let (name, inline) = match body.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (body, None),
            };
            if let Some(spec) = params.iter().find(|s| s.name == name) {
                if spec.kind == Kind::Flag {
                    let v = match inline {
                        Some(v) => spec.kind.parse(name, &v)?,
                        None => Value::Flag(true),
                    };
                    p.params.set(name, v);
                } else {
                    let raw = match inline {
                        Some(v) => v,
                        None => {
                            let v = args.get(i).cloned().ok_or_else(|| {
                                err(ErrorCode::BadOption, format!("--{name} needs a value"))
                            })?;
                            i += 1;
                            v
                        }
                    };
                    p.params.set(name, spec.kind.parse(name, &raw)?);
                }
                continue;
            }
            if let Some(flag) = name
                .strip_prefix("no-")
                .and_then(|n| params.iter().find(|s| s.name == n && s.kind == Kind::Flag))
            {
                p.params.set(flag.name, Value::Flag(false));
                continue;
            }
            if let Some(common) = COMMON.iter().find(|c| c.0 == name) {
                let value = if common.2 {
                    match inline {
                        Some(v) => Some(v),
                        None => {
                            let v = args.get(i).cloned();
                            i += 1;
                            v
                        }
                    }
                } else {
                    None
                };
                set_common(&mut p, name, value)?;
                continue;
            }
            return Err(unknown_option(name, params));
        }
        // Short options: -o value, -ovalue, -q, -j4.
        let body = &arg[1..];
        let short = body.chars().next().expect("non-empty");
        let Some(common) = COMMON.iter().find(|c| c.1 == Some(short)) else {
            return Err(err(ErrorCode::BadOption, format!("unknown option {arg}")));
        };
        let rest = &body[short.len_utf8()..];
        let value = if common.2 {
            if rest.is_empty() {
                let v = args.get(i).cloned();
                i += 1;
                v
            } else {
                Some(rest.to_string())
            }
        } else {
            if !rest.is_empty() {
                return Err(err(ErrorCode::BadOption, format!("unknown option {arg}")));
            }
            None
        };
        set_common(&mut p, common.0, value)?;
    }
    if std::env::var_os("DEFT_JSON").is_some_and(|v| v != "0") {
        p.json = true;
    }
    Ok(p)
}
