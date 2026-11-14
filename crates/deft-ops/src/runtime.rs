//! Runs an operation: expands inputs, fills defaults, plans output paths,
//! executes jobs in parallel and writes results atomically.

use crate::registry::registry;
use deft_core::error::{DeftError, ErrorCode, Result, err};
use deft_core::format::{Format, read_as_data, sniff, sniff_file};
use deft_core::job::{Ctx, Data, InputFile, Job, OpResult, Params, Produced};
use deft_core::spec::{Accept, InputArity, Op, OutputArity};
use deft_core::value::Value;
use serde_json::{Map, Value as Json};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

/// A fully parsed request to run one operation.
#[derive(Clone, Debug, Default)]
pub struct Request {
    pub inputs: Vec<String>,
    pub params: Params,
    pub output: Option<String>,
    pub force: bool,
    pub dry_run: bool,
    pub recursive: bool,
    pub jobs: usize,
    pub stdin: Option<Vec<u8>>,
    pub stdin_format: Option<Format>,
}

#[derive(Debug, Clone)]
pub struct OutputInfo {
    pub path: String,
    pub format: Option<Format>,
    pub bytes: Option<u64>,
    pub facts: Map<String, Json>,
}

/// Result of one job.
#[derive(Debug, Default)]
pub struct Item {
    pub inputs: Vec<String>,
    pub input_bytes: Option<u64>,
    pub outputs: Vec<OutputInfo>,
    pub facts: Map<String, Json>,
    pub note: String,
    pub warnings: Vec<String>,
    pub error: Option<DeftError>,
    /// Planned outputs (dry run only).
    pub planned: Vec<String>,
}

#[derive(Debug)]
pub struct Report {
    pub op: &'static Op,
    pub items: Vec<Item>,
    pub elapsed_ms: u128,
    pub dry_run: bool,
    /// Bytes for `-o -` (single output to stdout).
    pub stdout: Option<Vec<u8>>,
    /// Conversion chain used by `deft convert`, as op ids.
    pub chain: Vec<String>,
}

impl Report {
    pub fn ok(&self) -> bool {
        self.items.iter().all(|i| i.error.is_none())
    }
    pub fn exit_code(&self) -> i32 {
        let failed: Vec<_> = self.items.iter().filter_map(|i| i.error.as_ref()).collect();
        match failed.len() {
            0 => 0,
            n if n < self.items.len() => 5,
            _ => failed[0].code.exit_code(),
        }
    }
}

/// Fill defaults, derive `--to` from the output extension, and check required options.
pub fn finalize_params(op: &Op, params: &mut Params, output: Option<&str>) -> Result<()> {
    if let (Some(spec), Some(out)) = (op.param("to"), output)
        && !params.has("to")
        && out != "-"
        && !out.ends_with('/')
        && !Path::new(out).is_dir()
        && let Some(fmt) = Format::from_path(Path::new(out))
        && let deft_core::value::Kind::Choice(choices) = spec.kind
    {
        let ext = fmt.ext();
        let name = choices
            .iter()
            .find(|c| Format::from_name(c) == Some(fmt))
            .copied()
            .unwrap_or(ext);
        if choices.contains(&name) {
            params.set("to", Value::Text(name.to_string()));
        }
    }
    for p in op.params {
        if params.has(p.name) {
            continue;
        }
        if let Some(d) = p.default {
            params.set(p.name, p.kind.parse(p.name, d)?);
        } else if p.required {
            return Err(
                err(ErrorCode::BadOption, format!("--{} is required", p.name))
                    .hint(format!("deft {} --help", op.path())),
            );
        }
    }
    Ok(())
}

fn is_url(s: &str) -> bool {
    s.starts_with("http://") || s.starts_with("https://")
}

/// Turn raw input strings into files, expanding globs and directories.
fn expand_inputs(op: &Op, req: &Request) -> Result<Vec<InputFile>> {
    let mut out = Vec::new();
    if op.input.arity == InputArity::None {
        if !req.inputs.is_empty() {
            return Err(err(
                ErrorCode::BadOption,
                format!("deft {} takes no input files", op.path()),
            ));
        }
        return Ok(out);
    }
    for raw in &req.inputs {
        if op.input.accept == Accept::Url {
            if !is_url(raw) {
                return Err(err(
                    ErrorCode::InputUnsupported,
                    format!("{raw}: expected a web address (https://...)"),
                ));
            }
            out.push(InputFile::url(raw.clone()));
            continue;
        }
        if is_url(raw) {
            return Err(err(
                ErrorCode::InputUnsupported,
                format!("{raw}: deft {} works on local files", op.path()),
            )
            .hint("download it first"));
        }
        if raw == "-" {
            let bytes = req.stdin.clone().unwrap_or_default();
            let format = req
                .stdin_format
                .or_else(|| sniff(&bytes[..bytes.len().min(4096)], None));
            out.push(InputFile::stdin(bytes, format));
            continue;
        }
        let candidates: Vec<PathBuf> = if raw.contains(['*', '?', '[']) {
            let found: Vec<PathBuf> = glob::glob(raw)
                .map_err(|e| err(ErrorCode::BadValue, format!("bad pattern {raw}: {e}")))?
                .filter_map(|p| p.ok())
                .filter(|p| p.is_file())
                .collect();
            if found.is_empty() {
                return Err(err(
                    ErrorCode::InputNotFound,
                    format!("{raw}: no files match"),
                ));
            }
            found
        } else {
            let p = PathBuf::from(raw);
            if !p.exists() {
                return Err(err(
                    ErrorCode::InputNotFound,
                    format!("{raw}: no such file or directory"),
                ));
            }
            if p.is_dir() && op.input.accept != Accept::AnyFile {
                let mut files = Vec::new();
                walk(&p, req.recursive, &mut files)?;
                files.sort();
                let matching: Vec<PathBuf> = files
                    .into_iter()
                    .filter(|f| {
                        sniff_file(f)
                            .ok()
                            .flatten()
                            .is_some_and(|fmt| op.input.accepts(fmt))
                    })
                    .collect();
                if matching.is_empty() {
                    return Err(err(
                        ErrorCode::InputNotFound,
                        format!("{raw}: no {} files in this directory", op.input.label),
                    ));
                }
                matching
            } else {
                vec![p]
            }
        };
        for path in candidates {
            if path.is_dir() {
                out.push(InputFile::file(path, None));
                continue;
            }
            let mut format = sniff_file(&path)?;
            if let Accept::Formats(allowed) = op.input.accept {
                if !format.is_some_and(|f| allowed.contains(&f)) {
                    format = read_as_data(format, allowed, &head(&path)?).or(format);
                }
                match format {
                    Some(f) if allowed.contains(&f) => {}
                    Some(f) => {
                        if f == Format::Xls
                            && let Some(kind) = std::fs::read(&path)
                                .ok()
                                .and_then(|b| deft_core::format::ole_content(&b))
                        {
                            return Err(old_office(&path, kind, op));
                        }
                        return Err(err(
                            ErrorCode::InputMismatch,
                            format!(
                                "{}: expected {}, got {}",
                                path.display(),
                                op.input.label,
                                f.label()
                            ),
                        )
                        .hint(conversion_hint(f, op)));
                    }
                    None => {
                        return Err(err(
                            ErrorCode::InputUnsupported,
                            format!(
                                "{}: unknown file type, expected {}",
                                path.display(),
                                op.input.label
                            ),
                        ));
                    }
                }
            }
            out.push(InputFile::file(path, format));
        }
    }
    let n = out.len();
    match op.input.arity {
        InputArity::Each if n == 0 => Err(err(
            ErrorCode::BadOption,
            format!("deft {} needs an input", op.path()),
        )
        .hint(format!("deft {} <{}>", op.path(), op.input.label))),
        InputArity::All { min } if n < min => Err(err(
            ErrorCode::BadOption,
            format!("deft {} needs at least {min} inputs, got {n}", op.path()),
        )),
        _ => Ok(out),
    }
}

/// The first bytes of a file.
fn head(path: &Path) -> Result<Vec<u8>> {
    use std::io::Read;
    let mut buf = Vec::with_capacity(4096);
    std::fs::File::open(path)
        .and_then(|f| f.take(4096).read_to_end(&mut buf))
        .map_err(|e| err(ErrorCode::InputCorrupt, format!("{}: {e}", path.display())))?;
    Ok(buf)
}

/// Refuse an old or password-protected Office file with what to do about it.
fn old_office(path: &Path, kind: &str, op: &Op) -> DeftError {
    let (code, what, hint) = match kind {
        "encrypted" => (
            ErrorCode::InputEncrypted,
            "the document is password-protected",
            "remove the password in Office (File > Info > Protect), save, and try again",
        ),
        "word" => (
            ErrorCode::InputMismatch,
            "this is an old Word .doc file",
            "deft reads Word .docx; save the file as .docx first",
        ),
        _ => (
            ErrorCode::InputMismatch,
            "this is an old PowerPoint .ppt file",
            "deft reads .pptx; save the file as .pptx first",
        ),
    };
    let expected = if code == ErrorCode::InputMismatch {
        format!(", expected {}", op.input.label)
    } else {
        String::new()
    };
    err(code, format!("{}: {what}{expected}", path.display())).hint(hint)
}

fn conversion_hint(found: Format, op: &Op) -> String {
    if let Accept::Formats(allowed) = op.input.accept
        && let Some(target) = allowed.first()
        && registry().plan_conversion(found, *target).is_some()
    {
        return format!(
            "convert first: deft convert <file> -o <name>.{}",
            target.ext()
        );
    }
    format!("see deft find \"{} {}\"", found.label(), op.command())
}

fn walk(dir: &Path, recursive: bool, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let path = entry?.path();
        let hidden = path
            .file_name()
            .is_some_and(|n| n.to_string_lossy().starts_with('.'));
        if hidden {
            continue;
        }
        if path.is_dir() {
            if recursive {
                walk(&path, true, out)?;
            }
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// Output format the operation is expected to write, when known before running.
fn expected_format(op: &Op, params: &Params, input: Option<&InputFile>) -> Option<Format> {
    if op.param("to").is_some()
        && let Some(t) = params.opt_text("to")
        && let Some(f) = Format::from_name(&t)
    {
        return Some(f);
    }
    match op.output.formats {
        [only] => Some(*only),
        many => input.and_then(|i| i.format).filter(|f| many.contains(f)),
    }
}

/// Where outputs of one job go.
#[derive(Debug, Clone)]
enum Target {
    None,
    Stdout,
    /// Exact file path.
    File(PathBuf),
    /// Exact file path for a job that must yield exactly one output.
    Single(PathBuf),
    /// Directory; file names come from the outputs.
    Dir(PathBuf),
}

fn plan_target(
    op: &Op,
    req: &Request,
    job_inputs: &[InputFile],
    many_jobs: bool,
    expected: Option<Format>,
) -> Result<Target> {
    let first = job_inputs.first();
    let base_dir = first
        .and_then(|i| i.path.as_ref())
        .and_then(|p| p.parent())
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let stem = first.map(|i| i.stem()).unwrap_or_else(|| "output".into());
    let o = req.output.as_deref();
    let is_dir_arg = |o: &str| o.ends_with('/') || o.ends_with('\\') || Path::new(o).is_dir();
    Ok(match op.output.arity {
        OutputArity::Report if o.is_none() => Target::None,
        OutputArity::One | OutputArity::Report => match o {
            Some("-") if many_jobs => {
                return Err(err(
                    ErrorCode::BadOption,
                    "-o - writes one output; there are several inputs",
                ));
            }
            Some("-") => Target::Stdout,
            Some(o) if is_dir_arg(o) => Target::Dir(PathBuf::from(o)),
            Some(o) if many_jobs => {
                return Err(err(
                    ErrorCode::BadOption,
                    format!("-o {o}: several inputs need a directory, e.g. -o out/"),
                ));
            }
            Some(o) => Target::File(PathBuf::from(o)),
            None if first.is_some_and(|i| i.path.is_none() && i.url.is_none()) => Target::Stdout,
            None => {
                let dir = if first.is_some_and(|i| i.url.is_some()) {
                    PathBuf::new()
                } else {
                    base_dir
                };
                match expected {
                    // Known name up front, so OUTPUT_EXISTS can be checked before running.
                    Some(f) if first.is_some_and(|i| i.url.is_none()) => {
                        let name = default_name(op, &stem, f, first);
                        Target::File(dir.join(name))
                    }
                    _ => Target::Dir(dir),
                }
            }
        },
        OutputArity::Many => match o {
            Some("-") => {
                return Err(err(
                    ErrorCode::BadOption,
                    "-o - is not possible: this command writes several files",
                ));
            }
            // `-o cover.png` works when the job yields exactly one file.
            Some(o)
                if !is_dir_arg(o)
                    && Format::from_path(Path::new(o))
                        .is_some_and(|f| op.output.formats.contains(&f)) =>
            {
                Target::Single(PathBuf::from(o))
            }
            Some(o) => Target::Dir(PathBuf::from(o)),
            None if first.is_some_and(|i| i.url.is_some()) => Target::Dir(PathBuf::new()),
            None => {
                Target::Dir(base_dir.join(format!("{stem}-{}", op.suffix.unwrap_or(op.command()))))
            }
        },
        OutputArity::Dir => match o {
            Some("-") => {
                return Err(err(
                    ErrorCode::BadOption,
                    "-o - is not possible: this command writes a directory",
                ));
            }
            Some(o) if many_jobs => Target::Dir(PathBuf::from(o).join(&stem)),
            Some(o) => Target::File(PathBuf::from(o)),
            None => Target::File(base_dir.join(&stem)),
        },
    })
}

fn default_name(op: &Op, stem: &str, out: Format, input: Option<&InputFile>) -> String {
    let format = input.and_then(|i| i.format);
    // A conversion keeps the stem; a same-format edit adds a suffix so the
    // input is never overwritten.
    if op.suffix.is_some() || format == Some(out) {
        // A variant deft has no name for, such as a macro-enabled .docm or a
        // .dotx template, keeps its extension: the file is still one.
        let own = input
            .and_then(|i| i.path.as_deref())
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .filter(|e| format == Some(out) && Format::from_name(e).is_none());
        format!(
            "{stem}.{}.{}",
            op.suffix.unwrap_or(op.command()),
            own.unwrap_or(out.ext())
        )
    } else {
        format!("{stem}.{}", out.ext())
    }
}

/// Whether two paths are one file: the same path, a link to it, or a hard link.
fn same_file(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if let (Ok(x), Ok(y)) = (std::fs::metadata(a), std::fs::metadata(b)) {
            return x.dev() == y.dev() && x.ino() == y.ino();
        }
    }
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

/// Check a planned path before running.
fn check_writable(path: &Path, inputs: &[InputFile], req: &Request) -> Result<()> {
    let is_input = inputs
        .iter()
        .any(|i| i.path.as_deref().is_some_and(|p| same_file(p, path)));
    // An input is never written to, whatever the options.
    if is_input {
        return Err(err(
            ErrorCode::OutputExists,
            format!(
                "{} is an input; deft never changes its inputs",
                path.display()
            ),
        )
        .hint("choose another -o"));
    }
    if path.exists() && !req.force && !is_input {
        return Err(err(
            ErrorCode::OutputExists,
            format!("{} already exists", path.display()),
        )
        .hint("pass --force to overwrite, or choose another -o"));
    }
    Ok(())
}

/// The format implied by `-o name.ext`. Refuses extensions the operation
/// cannot produce, so a file's name always matches its contents.
fn requested_format(op: &Op, output: Option<&str>) -> Result<Option<Format>> {
    let Some(o) = output
        .filter(|o| *o != "-" && !o.ends_with('/') && !o.ends_with('\\') && !Path::new(o).is_dir())
    else {
        return Ok(None);
    };
    let report = op.output.arity == OutputArity::Report;
    if !report && (op.output.arity != OutputArity::One || op.output.formats.is_empty()) {
        return Ok(None);
    }
    let Some(f) = Format::from_path(Path::new(o)) else {
        return Ok(None);
    };
    if report && f != Format::Json {
        return Err(err(
            ErrorCode::BadOption,
            format!("-o {o}: deft {} writes its result as json", op.path()),
        ));
    }
    if report {
        return Ok(None);
    }
    if !op.output.formats.contains(&f) {
        let can: Vec<&str> = op.output.formats.iter().map(|f| f.ext()).collect();
        return Err(err(
            ErrorCode::BadOption,
            format!("-o {o}: deft {} writes {}", op.path(), can.join(", ")),
        )
        .hint("for other formats: deft convert <input> -o <name.ext>"));
    }
    Ok(Some(f))
}

/// Where a worker leaves the result of one job.
type Slot = Mutex<Option<(Result<OpResult>, Vec<InputFile>)>>;

/// Execute a request.
pub fn execute(op: &'static Op, mut req: Request, ctx: &Ctx) -> Result<Report> {
    let started = Instant::now();
    let inputs = expand_inputs(op, &req)?;
    finalize_params(op, &mut req.params, req.output.as_deref())?;
    let out_format = requested_format(op, req.output.as_deref())?;
    let jobs: Vec<Vec<InputFile>> = match op.input.arity {
        InputArity::Each => inputs.into_iter().map(|i| vec![i]).collect(),
        InputArity::All { .. } | InputArity::None => vec![inputs],
    };
    let many_jobs = jobs.len() > 1;

    // Plan targets and check them before doing any work.
    let mut planned = Vec::new();
    for job in &jobs {
        let expected = expected_format(op, &req.params, job.first());
        let target = plan_target(op, &req, job, many_jobs, expected)?;
        if let Target::File(p) | Target::Single(p) = &target
            && (op.output.arity != OutputArity::Dir || !req.force)
        {
            check_writable(p, job, &req)?;
        }
        planned.push(target);
    }

    if req.dry_run {
        let items = jobs
            .iter()
            .zip(&planned)
            .map(|(job, t)| Item {
                inputs: job.iter().map(|i| i.display.clone()).collect(),
                planned: vec![match t {
                    Target::None => "(report)".into(),
                    Target::Stdout => "(stdout)".into(),
                    Target::File(p) | Target::Single(p) => p.display().to_string(),
                    Target::Dir(d) => format!("{}/", d.display().to_string().trim_end_matches('/')),
                }],
                ..Default::default()
            })
            .collect();
        return Ok(Report {
            op,
            items,
            elapsed_ms: started.elapsed().as_millis(),
            dry_run: true,
            stdout: None,
            chain: vec![],
        });
    }

    let params = &req.params;
    let out_dirs: Vec<Option<PathBuf>> = planned
        .iter()
        .map(|t| match t {
            Target::File(p) | Target::Single(p) => {
                Some(p.parent().map(Path::to_path_buf).unwrap_or_default())
            }
            Target::Dir(d) => Some(d.clone()),
            Target::None | Target::Stdout => None,
        })
        .collect();
    let results: Vec<Slot> = jobs.iter().map(|_| Mutex::new(None)).collect();
    let queue: Mutex<Vec<(usize, Vec<InputFile>)>> =
        Mutex::new(jobs.into_iter().enumerate().rev().collect());
    let workers = req.jobs.max(1).min(results.len().max(1));
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let next = queue.lock().unwrap().pop();
                    let Some((index, inputs)) = next else { break };
                    let job = Job {
                        inputs,
                        params,
                        ctx,
                        out_format,
                        out_dir: out_dirs[index].clone(),
                    };
                    let outcome =
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (op.run)(&job)))
                            .unwrap_or_else(|panic| {
                                Err(err(
                                    ErrorCode::Internal,
                                    format!("crashed: {}", panic_message(&panic)),
                                ))
                            });
                    *results[index].lock().unwrap() = Some((outcome, job.inputs));
                }
            });
        }
    });

    let mut items = Vec::new();
    let mut stdout = None;
    for (slot, target) in results.into_iter().zip(planned) {
        let (outcome, inputs) = slot.into_inner().unwrap().expect("every job ran");
        let mut item = Item {
            inputs: inputs.iter().map(|i| i.display.clone()).collect(),
            input_bytes: inputs
                .iter()
                .map(|i| {
                    i.path
                        .as_ref()
                        .and_then(|p| std::fs::metadata(p).ok())
                        .map(|m| m.len())
                })
                .sum(),
            ..Default::default()
        };
        match outcome.and_then(|r| deliver(op, r, &target, &inputs, &req, &mut stdout)) {
            Ok((res, outs)) => {
                item.outputs = outs;
                item.facts = res.facts;
                item.note = res.note;
                item.warnings = res.warnings;
            }
            Err(e) => item.error = Some(e),
        }
        items.push(item);
    }
    Ok(Report {
        op,
        items,
        elapsed_ms: started.elapsed().as_millis(),
        dry_run: false,
        stdout,
        chain: vec![],
    })
}

fn panic_message(p: &Box<dyn std::any::Any + Send>) -> String {
    p.downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| p.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "panic".into())
}

/// Write a job's outputs to their final place.
fn deliver(
    op: &Op,
    mut res: OpResult,
    target: &Target,
    inputs: &[InputFile],
    req: &Request,
    stdout: &mut Option<Vec<u8>>,
) -> Result<(OpResult, Vec<OutputInfo>)> {
    // A report has no output of its own; -o asks for its facts as a file.
    if op.output.arity == OutputArity::Report && !matches!(target, Target::None) {
        let mut json = serde_json::to_vec_pretty(&res.facts)
            .map_err(|e| err(ErrorCode::Internal, e.to_string()))?;
        json.push(b'\n');
        res.outputs = vec![Produced::bytes(json, Format::Json)];
    }
    let outputs = std::mem::take(&mut res.outputs);
    let stem = inputs
        .first()
        .map(|i| i.stem())
        .unwrap_or_else(|| "output".into());
    let mut infos = Vec::new();
    let count = outputs.len();
    for (n, p) in outputs.into_iter().enumerate() {
        let dest = match target {
            Target::None => continue,
            Target::Stdout => {
                match p.data {
                    Data::Bytes(b) => *stdout = Some(b),
                    Data::File(f) => *stdout = Some(std::fs::read(f)?),
                    Data::Dir(_) => {
                        return Err(err(
                            ErrorCode::BadOption,
                            "cannot write a directory to stdout",
                        ));
                    }
                }
                infos.push(OutputInfo {
                    path: "-".into(),
                    format: p.format,
                    bytes: stdout.as_ref().map(|b| b.len() as u64),
                    facts: p.facts,
                });
                continue;
            }
            Target::Single(f) if count != 1 => {
                return Err(err(
                    ErrorCode::BadOption,
                    format!(
                        "-o {}: this job writes {count} files; use a directory, e.g. -o out/",
                        f.display()
                    ),
                ));
            }
            Target::File(f) | Target::Single(f) => {
                // A file's name must match its contents.
                if let (Some(want), Some(got)) = (Format::from_path(f), p.format)
                    && want != got
                    && op.output.arity == OutputArity::One
                {
                    return Err(err(
                        ErrorCode::BadOption,
                        format!(
                            "{}: the name says {want}, but deft {} writes {got} here",
                            f.display(),
                            op.path()
                        ),
                    )
                    .hint("use a matching extension, or change --to"));
                }
                f.clone()
            }
            Target::Dir(d) => {
                let name = p.name.clone().unwrap_or_else(|| {
                    let ext = p.format.map(|f| f.ext()).unwrap_or("bin");
                    if count > 1 {
                        format!("{stem}-{:03}.{ext}", n + 1)
                    } else if req.output.is_some() {
                        // An explicit -o dir/ keeps the original file name.
                        format!("{stem}.{ext}")
                    } else {
                        default_name(op, &stem, p.format.unwrap_or(Format::Txt), inputs.first())
                    }
                });
                let dest = d.join(sanitize(&name));
                check_writable(&dest, inputs, req)?;
                dest
            }
        };
        if let Some(parent) = dest.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent).map_err(|e| {
                err(
                    ErrorCode::OutputUnwritable,
                    format!("{}: {e}", parent.display()),
                )
            })?;
        }
        let bytes = place(p.data, &dest, req.force, inputs)?;
        infos.push(OutputInfo {
            path: dest.display().to_string(),
            format: p.format,
            bytes,
            facts: p.facts,
        });
    }
    Ok((res, infos))
}

/// Keep generated names to safe file names.
pub fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    let trimmed = cleaned.trim().trim_start_matches('.');
    if trimmed.is_empty() {
        "output".into()
    } else {
        trimmed.to_string()
    }
}

/// Move data into place atomically (temp file + rename).
fn place(data: Data, dest: &Path, overwrite: bool, inputs: &[InputFile]) -> Result<Option<u64>> {
    let is_input = |p: &Path| {
        inputs
            .iter()
            .any(|i| i.path.as_deref().is_some_and(|q| same_file(q, p)))
    };
    if is_input(dest) {
        return Err(err(
            ErrorCode::OutputExists,
            format!(
                "{} is an input; deft never changes its inputs",
                dest.display()
            ),
        ));
    }
    let unwritable = |e: std::io::Error| {
        err(
            ErrorCode::OutputUnwritable,
            format!("{}: {e}", dest.display()),
        )
    };
    let tmp = dest.with_file_name(format!(
        ".{}.deft-tmp",
        dest.file_name()
            .map(|n| n.to_string_lossy())
            .unwrap_or_default()
    ));
    match data {
        Data::Bytes(b) => {
            std::fs::write(&tmp, &b).map_err(unwritable)?;
            std::fs::rename(&tmp, dest).map_err(unwritable)?;
            Ok(Some(b.len() as u64))
        }
        Data::File(src) => {
            if std::fs::rename(&src, dest).is_err() {
                std::fs::copy(&src, &tmp).map_err(unwritable)?;
                std::fs::rename(&tmp, dest).map_err(unwritable)?;
            }
            Ok(std::fs::metadata(dest).ok().map(|m| m.len()))
        }
        Data::Dir(src) => {
            if dest.exists() {
                if !overwrite {
                    return Err(err(
                        ErrorCode::OutputExists,
                        format!("{} already exists", dest.display()),
                    )
                    .hint("pass --force to merge into it"));
                }
                copy_dir(&src, dest, &is_input).map_err(unwritable)?;
            } else if std::fs::rename(&src, dest).is_err() {
                copy_dir(&src, dest, &is_input).map_err(unwritable)?;
            }
            Ok(None)
        }
    }
}

/// Copy a folder into place, merging with one already there. An input in the
/// way is never replaced.
fn copy_dir(src: &Path, dest: &Path, is_input: &dyn Fn(&Path) -> bool) -> std::io::Result<()> {
    std::fs::create_dir_all(dest)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let to = dest.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&entry.path(), &to, is_input)?;
        } else if is_input(&to) {
            return Err(std::io::Error::other(format!(
                "{} is an input; deft never changes its inputs",
                to.display()
            )));
        } else {
            std::fs::copy(entry.path(), to)?;
        }
    }
    Ok(())
}

/// `deft convert`: pick a chain of operations by the input and output formats.
pub fn convert(req: Request, to: Option<String>, ctx: &Ctx) -> Result<Report> {
    let started = Instant::now();
    let reg = registry();
    let target = match (&to, req.output.as_deref()) {
        (Some(t), _) => Format::from_name(t)
            .ok_or_else(|| err(ErrorCode::BadValue, format!("--to: unknown format {t:?}")))?,
        (None, Some(o)) if o != "-" => Format::from_path(Path::new(o)).ok_or_else(|| {
            err(
                ErrorCode::BadOption,
                format!("-o {o}: cannot tell the target format; add --to <format>"),
            )
        })?,
        _ => {
            return Err(err(
                ErrorCode::BadOption,
                "deft convert needs --to <format> or -o <file.ext>",
            )
            .hint("deft convert photo.heic -o photo.jpg"));
        }
    };
    // A pseudo-op used for reporting.
    static CONVERT: Op = Op {
        id: "convert",
        summary: "Any-to-any conversion by extension",
        ..Op::DEFAULT
    };
    let any = Op {
        input: deft_core::spec::Input::any_files(1),
        ..Op::DEFAULT
    };
    let any: &'static Op = Box::leak(Box::new(any));
    let inputs = expand_inputs(any, &req)?;
    let many = inputs.len() > 1;
    let mut items = Vec::new();
    let mut chain_ids = Vec::new();
    let mut stdout_bytes: Option<Vec<u8>> = None;
    for input in inputs {
        let mut item = Item {
            inputs: vec![input.display.clone()],
            ..Default::default()
        };
        let outcome = (|| -> Result<(Vec<OutputInfo>, Vec<String>, Vec<String>)> {
            let from = input.require_format()?;
            let chain = reg.plan_conversion(from, target).ok_or_else(|| {
                let reach: Vec<&str> = reg.reachable(from).iter().map(|f| f.ext()).collect();
                err(
                    ErrorCode::NoConversionPath,
                    format!("no converter from {} to {}", from.label(), target.label()),
                )
                .hint(if reach.is_empty() {
                    "this format cannot be converted yet".to_string()
                } else {
                    format!("{} converts to: {}", from.label(), reach.join(", "))
                })
            })?;
            if chain.is_empty() {
                return Err(err(
                    ErrorCode::BadOption,
                    format!("{} is already {}", input.display, target.label()),
                ));
            }
            let ids: Vec<String> = chain.iter().map(|(op, _)| op.id.to_string()).collect();
            // Options for a step, such as --images for a conversion to Markdown,
            // go to the steps that have them. Pictures of a Markdown file in the
            // middle of a chain go to a scratch folder, so --images belongs to
            // the last step only.
            let applies = |name: &str| {
                chain.iter().enumerate().any(|(i, (op, _))| {
                    op.param(name).is_some() && (name != "images" || i + 1 == chain.len())
                })
            };
            if let Some((name, _)) = req.params.iter().find(|(n, _)| *n != "to" && !applies(n)) {
                return Err(err(
                    ErrorCode::BadOption,
                    format!(
                        "--{name} does not apply to {} to {} (via {})",
                        from.label(),
                        target.label(),
                        ids.join(" > ")
                    ),
                ));
            }
            let mut warnings = Vec::new();
            if chain.iter().any(|(op, _)| {
                op.converts
                    .iter()
                    .any(|e| e.quality == deft_core::spec::Quality::Approximate)
            }) {
                warnings.push(
                    "approximate conversion: layout may differ from the original".to_string(),
                );
            }
            let stem = input.stem();
            let to_dir = req
                .output
                .as_deref()
                .is_some_and(|o| o.ends_with('/') || Path::new(o).is_dir());
            let dest = match req.output.as_deref() {
                Some("-") => None,
                Some(o) if o.ends_with('/') || Path::new(o).is_dir() => {
                    Some(PathBuf::from(o).join(format!("{stem}.{}", target.ext())))
                }
                Some(o) if many => {
                    return Err(err(
                        ErrorCode::BadOption,
                        format!("-o {o}: several inputs need a directory, e.g. -o out/"),
                    ));
                }
                Some(o) => Some(PathBuf::from(o)),
                None => Some(
                    input
                        .path
                        .as_ref()
                        .and_then(|p| p.parent())
                        .map(Path::to_path_buf)
                        .unwrap_or_default()
                        .join(format!("{stem}.{}", target.ext())),
                ),
            };
            if let Some(d) = &dest {
                check_writable(d, std::slice::from_ref(&input), &req)?;
            }
            if req.dry_run {
                return Ok((
                    vec![OutputInfo {
                        path: dest.map(|d| d.display().to_string()).unwrap_or("-".into()),
                        format: Some(target),
                        bytes: None,
                        facts: Map::new(),
                    }],
                    ids,
                    warnings,
                ));
            }
            // Run each step on a temp file.
            let mut current = input.local_path(ctx)?;
            let mut current_format = from;
            for (step, (op, step_to)) in chain.iter().enumerate() {
                let mut params = Params::default();
                if op.param("to").is_some() {
                    params.set("to", Value::Text(step_to.ext().to_string()));
                }
                let last = step + 1 == chain.len();
                for (name, value) in req.params.iter() {
                    if name != "to" && op.param(name).is_some() {
                        params.set(name, value.clone());
                    }
                }
                if !last && *step_to == Format::Md && op.param("images").is_some() {
                    params.set(
                        "images",
                        Value::Path(ctx.temp_path(&format!("images-{step}"))),
                    );
                }
                finalize_params(op, &mut params, None)?;
                let job = Job {
                    inputs: vec![InputFile::file(current.clone(), Some(current_format))],
                    params: &params,
                    ctx,
                    out_format: Some(*step_to),
                    out_dir: None,
                };
                let mut res = (op.run)(&job)?;
                warnings.append(&mut res.warnings);
                let last = step + 1 == chain.len();
                let produced: Produced = match res.outputs.len() {
                    1 => res.outputs.remove(0),
                    n if last && to_dir => {
                        let dir = dest
                            .as_ref()
                            .and_then(|d| d.parent())
                            .unwrap_or(Path::new(""));
                        std::fs::create_dir_all(dir)?;
                        let mut infos = Vec::new();
                        for (i, p) in res.outputs.into_iter().enumerate() {
                            let name = p
                                .name
                                .clone()
                                .unwrap_or_else(|| format!("{stem}-{:03}.{}", i + 1, target.ext()));
                            let path = dir.join(sanitize(&name));
                            check_writable(&path, std::slice::from_ref(&input), &req)?;
                            let (format, facts) = (p.format, p.facts);
                            let bytes = place(p.data, &path, true, std::slice::from_ref(&input))?;
                            infos.push(OutputInfo {
                                path: path.display().to_string(),
                                format,
                                bytes,
                                facts,
                            });
                        }
                        debug_assert_eq!(infos.len(), n);
                        return Ok((infos, ids, warnings));
                    }
                    0 => {
                        return Err(err(
                            ErrorCode::Internal,
                            format!("{} produced nothing", op.id),
                        ));
                    }
                    n => {
                        return Err(err(
                            ErrorCode::BadOption,
                            format!(
                                "{} produces {n} files; run deft {} directly",
                                op.id,
                                op.path()
                            ),
                        ));
                    }
                };
                let next = ctx.temp_path(&format!("{stem}.{}", step_to.ext()));
                place(produced.data, &next, true, &[])?;
                current = next;
                current_format = *step_to;
            }
            let info = match dest {
                None => {
                    let bytes = std::fs::read(&current)?;
                    let len = bytes.len() as u64;
                    stdout_bytes = Some(bytes);
                    OutputInfo {
                        path: "-".into(),
                        format: Some(target),
                        bytes: Some(len),
                        facts: Map::new(),
                    }
                }
                Some(d) => {
                    if let Some(parent) = d.parent().filter(|p| !p.as_os_str().is_empty()) {
                        std::fs::create_dir_all(parent)?;
                    }
                    let bytes = place(Data::File(current), &d, true, std::slice::from_ref(&input))?;
                    OutputInfo {
                        path: d.display().to_string(),
                        format: Some(target),
                        bytes,
                        facts: Map::new(),
                    }
                }
            };
            Ok((vec![info], ids, warnings))
        })();
        match outcome {
            Ok((outs, ids, warnings)) => {
                if req.dry_run {
                    item.planned = outs.iter().map(|o| o.path.clone()).collect();
                } else {
                    item.outputs = outs;
                }
                item.note = format!("via {}", ids.join(" > "));
                item.warnings = warnings;
                chain_ids = ids;
            }
            Err(e) => item.error = Some(e),
        }
        items.push(item);
    }
    Ok(Report {
        op: &CONVERT,
        items,
        elapsed_ms: started.elapsed().as_millis(),
        dry_run: req.dry_run,
        stdout: stdout_bytes,
        chain: chain_ids,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_keep_an_extension_deft_has_no_name_for() {
        let reg = registry();
        let op = reg.ops.iter().find(|o| o.id == "doc.replace").unwrap();
        let file = |name: &str| InputFile::file(PathBuf::from(name), Some(Format::Docx));
        let name = |f: &InputFile| default_name(op, &f.stem(), Format::Docx, Some(f));
        assert_eq!(name(&file("a.docm")), "a.replaced.docm");
        assert_eq!(name(&file("t.dotx")), "t.replaced.dotx");
        assert_eq!(name(&file("a.docx")), "a.replaced.docx");
        // Content that is not what its extension says gets the right one.
        assert_eq!(name(&file("a.zip")), "a.replaced.docx");
    }

    /// Run `deft convert` on one file.
    fn convert_file(input: &Path, out: &Path, params: Params) -> Result<Report> {
        let tmp = tempfile::tempdir().unwrap();
        let req = Request {
            inputs: vec![input.display().to_string()],
            output: Some(out.display().to_string()),
            params,
            jobs: 1,
            ..Default::default()
        };
        convert(req, None, &Ctx::new(tmp.path().to_path_buf()))
    }

    #[test]
    fn pictures_survive_a_chain_through_markdown() {
        let dir = tempfile::tempdir().unwrap();
        let png = deft_tools_image::encode(
            &deft_tools_image::image::DynamicImage::new_rgb8(40, 30),
            Format::Png,
            Default::default(),
        )
        .unwrap();
        std::fs::write(dir.path().join("pic.png"), png).unwrap();
        let md = dir.path().join("a.md");
        std::fs::write(&md, "# Title\n\n![a picture](pic.png)\n").unwrap();
        let docx = dir.path().join("a.docx");
        let r = convert_file(&md, &docx, Params::default()).unwrap();
        assert!(r.items[0].error.is_none(), "{:?}", r.items[0].error);
        // docx to epub runs doc.to-md, then md.to-epub.
        let epub = dir.path().join("a.epub");
        let r = convert_file(&docx, &epub, Params::default()).unwrap();
        assert_eq!(r.chain, ["doc.to-md", "md.to-epub"]);
        let bytes = std::fs::read(&epub).unwrap();
        let book = deft_tools_doc::epub::read(&bytes).unwrap();
        assert_eq!(book.images.len(), 1, "the picture was lost");
        // A step's own option is accepted, and one no step has is refused.
        let mut images = Params::default();
        images.set("images", Value::Path(dir.path().join("media")));
        let r = convert_file(&epub, &dir.path().join("b.md"), images.clone()).unwrap();
        assert!(r.items[0].error.is_none(), "{:?}", r.items[0].error);
        assert_eq!(
            std::fs::read_dir(dir.path().join("media")).unwrap().count(),
            1
        );
        let r = convert_file(&docx, &dir.path().join("c.pdf"), images).unwrap();
        let e = r.items[0].error.as_ref().unwrap();
        assert_eq!(e.code, ErrorCode::BadOption);
    }

    #[test]
    fn old_and_locked_office_files_say_so() {
        let reg = registry();
        let op = reg.ops.iter().find(|o| o.id == "doc.to-text").unwrap();
        let ole = |stream: &str| {
            let mut b = vec![0xD0, 0xCF, 0x11, 0xE0, 0, 0];
            b.extend(stream.encode_utf16().flat_map(u16::to_le_bytes));
            b
        };
        let kind = |s: &str| deft_core::format::ole_content(&ole(s));
        assert_eq!(kind("EncryptedPackage"), Some("encrypted"));
        assert_eq!(kind("WordDocument"), Some("word"));
        assert_eq!(kind("Workbook"), None);
        let e = old_office(Path::new("a.docx"), "encrypted", op);
        assert_eq!(e.code, ErrorCode::InputEncrypted);
        assert_eq!(e.message, "a.docx: the document is password-protected");
        let e = old_office(Path::new("a.doc"), "word", op);
        assert_eq!(e.code, ErrorCode::InputMismatch);
        assert!(e.message.contains("old Word .doc"), "{}", e.message);
    }
}
