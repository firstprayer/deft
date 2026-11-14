//! External engines: locating and running programs such as ffmpeg. Engines are never invoked through a shell; arguments are passed
//! as an array. An engine can be overridden with `DEFT_ENGINE_<NAME>=<path>`.

use deft_core::error::{ErrorCode, Result, err};
use deft_core::job::Ctx;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy)]
pub struct Engine {
    pub name: &'static str,
    pub purpose: &'static str,
    /// Argument that prints the version.
    pub version_arg: &'static str,
    /// How to install it by hand.
    pub manual: &'static str,
}

const VERSION_TIMEOUT: Duration = Duration::from_secs(5);

pub const ENGINES: &[Engine] = &[
    Engine {
        name: "ffmpeg",
        purpose: "video and audio processing",
        version_arg: "-version",
        manual: "brew install ffmpeg | apt install ffmpeg | winget install ffmpeg",
    },
    Engine {
        name: "ffprobe",
        purpose: "video and audio inspection (ships with ffmpeg)",
        version_arg: "-version",
        manual: "brew install ffmpeg | apt install ffmpeg | winget install ffmpeg",
    },
    Engine {
        name: "tesseract",
        purpose: "reading text in images (OCR)",
        version_arg: "--version",
        manual: "brew install tesseract | apt install tesseract-ocr | winget install tesseract",
    },
];

pub fn engine(name: &str) -> Option<&'static Engine> {
    ENGINES.iter().find(|e| e.name == name)
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

fn env_key(name: &str) -> String {
    format!(
        "DEFT_ENGINE_{}",
        name.to_ascii_uppercase().replace('-', "_")
    )
}

/// Find an engine: explicit override, then managed install, then PATH.
pub fn locate(ctx: &Ctx, name: &str) -> Option<PathBuf> {
    if let Some(p) = std::env::var_os(env_key(name)) {
        // An empty override hides the engine (used by tests).
        return if p.is_empty() {
            None
        } else {
            Some(PathBuf::from(p))
        };
    }
    let managed = ctx.engine_dir.join(exe(name));
    if managed.is_file() {
        return Some(managed);
    }
    let path = std::env::var_os("PATH")?;
    let dirs: Vec<PathBuf> = std::env::split_paths(&path).collect();
    dirs.iter().map(|d| d.join(exe(name))).find(|p| p.is_file())
}

/// Find an engine or fail with ENGINE_MISSING.
pub fn require(ctx: &Ctx, name: &str, needed_by: &str) -> Result<PathBuf> {
    if let Some(p) = locate(ctx, name) {
        return Ok(p);
    }
    let def =
        engine(name).ok_or_else(|| err(ErrorCode::Internal, format!("unknown engine {name}")))?;
    Err(err(
        ErrorCode::EngineMissing,
        format!("{needed_by} needs {name} (not found)"),
    )
    .hint(format!("install it: {}", def.manual)))
}

pub fn version(path: &Path, version_arg: &str) -> Option<String> {
    version_within(path, version_arg, VERSION_TIMEOUT)
}

/// A program that does not answer in time has no version, so a stuck engine
/// cannot stop `deft doctor`.
fn version_within(path: &Path, version_arg: &str, limit: Duration) -> Option<String> {
    let mut child = Command::new(path)
        .arg(version_arg)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + limit;
    while child.try_wait().ok()?.is_none() {
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let mut out = Vec::new();
    child.stdout.take()?.read_to_end(&mut out).ok()?;
    let text = String::from_utf8_lossy(&out);
    text.lines()
        .next()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
}

#[derive(Debug)]
pub struct RunOutput {
    pub stdout: Vec<u8>,
    pub stderr: String,
}

/// Run an engine and capture its output. Non-zero exit becomes ENGINE_FAILED
/// with the tail of stderr; `--timeout` becomes TIMEOUT.
pub fn run(ctx: &Ctx, program: &Path, args: &[String]) -> Result<RunOutput> {
    use std::io::Read;
    let name = program
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            err(
                ErrorCode::EngineFailed,
                format!("could not start {name}: {e}"),
            )
        })?;
    let mut out_pipe = child.stdout.take().expect("piped");
    let mut err_pipe = child.stderr.take().expect("piped");
    let out_thread = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = out_pipe.read_to_end(&mut b);
        b
    });
    let err_thread = std::thread::spawn(move || {
        let mut b = Vec::new();
        let _ = err_pipe.read_to_end(&mut b);
        b
    });
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {}
            Err(e) => return Err(err(ErrorCode::EngineFailed, format!("{name}: {e}"))),
        }
        if ctx.timeout.is_some_and(|t| started.elapsed() > t) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(err(
                ErrorCode::Timeout,
                format!("{name} exceeded --timeout"),
            ));
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let stdout = out_thread.join().unwrap_or_default();
    let stderr = String::from_utf8_lossy(&err_thread.join().unwrap_or_default()).to_string();
    if !status.success() {
        // A crash can leave stderr empty; the status still says what happened.
        let why = match tail(&stderr, 6) {
            t if t.trim().is_empty() => status.to_string(),
            t => t,
        };
        return Err(err(
            ErrorCode::EngineFailed,
            format!("{name} failed: {why}"),
        ));
    }
    Ok(RunOutput { stdout, stderr })
}

/// Last `lines` non-empty lines, joined.
pub fn tail(text: &str, lines: usize) -> String {
    let v: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    v[v.len().saturating_sub(lines)..].join(" | ")
}

#[cfg(test)]
#[path = "lib_tests.rs"]
mod tests;

/// Base command for tesseract: the image, an output base, then the languages.
fn tesseract_args(image: &Path, output: &str, lang: &str) -> Vec<String> {
    vec![
        image.display().to_string(),
        output.to_string(),
        "-l".to_string(),
        lang.to_string(),
    ]
}

/// Language codes of the tesseract models (tessdata), plus `script/...`.
const TESSERACT_LANGS: &[&str] = &[
    "afr",
    "amh",
    "ara",
    "asm",
    "aze",
    "aze_cyrl",
    "bel",
    "ben",
    "bod",
    "bos",
    "bre",
    "bul",
    "cat",
    "ceb",
    "ces",
    "chi_sim",
    "chi_sim_vert",
    "chi_tra",
    "chi_tra_vert",
    "chr",
    "cos",
    "cym",
    "dan",
    "deu",
    "deu_latf",
    "div",
    "dzo",
    "ell",
    "eng",
    "enm",
    "epo",
    "equ",
    "est",
    "eus",
    "fao",
    "fas",
    "fil",
    "fin",
    "fra",
    "frk",
    "frm",
    "fry",
    "gla",
    "gle",
    "glg",
    "grc",
    "guj",
    "hat",
    "heb",
    "hin",
    "hrv",
    "hun",
    "hye",
    "iku",
    "ind",
    "isl",
    "ita",
    "ita_old",
    "jav",
    "jpn",
    "jpn_vert",
    "kan",
    "kat",
    "kat_old",
    "kaz",
    "khm",
    "kir",
    "kmr",
    "kor",
    "kor_vert",
    "lao",
    "lat",
    "lav",
    "lit",
    "ltz",
    "mal",
    "mar",
    "mkd",
    "mlt",
    "mon",
    "mri",
    "msa",
    "mya",
    "nep",
    "nld",
    "nor",
    "oci",
    "ori",
    "osd",
    "pan",
    "pol",
    "por",
    "pus",
    "que",
    "ron",
    "rus",
    "san",
    "sin",
    "slk",
    "slv",
    "snd",
    "snum",
    "spa",
    "spa_old",
    "sqi",
    "srp",
    "srp_latn",
    "sun",
    "swa",
    "swe",
    "syr",
    "tam",
    "tat",
    "tel",
    "tgk",
    "tha",
    "tir",
    "ton",
    "tur",
    "uig",
    "ukr",
    "urd",
    "uzb",
    "uzb_cyrl",
    "vie",
    "yid",
    "yor",
];

/// Check `--lang`: tesseract codes joined by `+`. A code tesseract does not
/// have would otherwise reach it and come back as a missing language pack.
pub fn check_tesseract_lang(lang: &str) -> std::result::Result<(), String> {
    for code in lang.split('+') {
        let script = code.strip_prefix("script/").is_some_and(|s| {
            !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        });
        if !script && !TESSERACT_LANGS.contains(&code) {
            return Err(format!(
                "{code:?} is not a tesseract language; use codes such as eng, deu, chi_sim, joined by +"
            ));
        }
    }
    Ok(())
}

/// Explain the common failure: a language pack that is not installed, which
/// is a missing part of the engine, not an engine error.
fn tesseract_error(mut e: deft_core::error::DeftError, lang: &str) -> deft_core::error::DeftError {
    if e.message.contains("Failed loading language") || e.message.contains("Error opening data") {
        e.code = ErrorCode::EngineMissing;
        e.message = format!("tesseract has no language data for {lang}");
        let pack = lang.replace('+', " tesseract-ocr-");
        e.hint = Some(format!(
            "install it: brew install tesseract-lang | apt install tesseract-ocr-{pack}"
        ));
    }
    e
}

/// Recognised text of one image.
pub fn ocr_text(ctx: &Ctx, image: &Path, lang: &str, needed_by: &str) -> Result<String> {
    let program = require(ctx, "tesseract", needed_by)?;
    let out = run(ctx, &program, &tesseract_args(image, "stdout", lang))
        .map_err(|e| tesseract_error(e, lang))?;
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// What tesseract read on one page image.
pub struct OcrPage {
    pub text: String,
    /// The page holding only the invisible text, when one was asked for.
    pub layer: Option<PathBuf>,
    /// Tesseract could not load its orientation data (osd), so a page turned
    /// sideways or upside down was read as it is.
    pub no_orientation: bool,
}

/// Read one page image, turning it upright first when tesseract finds it on
/// its side or upside down (page segmentation mode 1, which needs the osd
/// data that ships with tesseract). The image is at `dpi`. With `layer`, also
/// write the invisible text as a PDF page; text and layer come from one reading.
pub fn ocr_page(
    ctx: &Ctx,
    image: &Path,
    lang: &str,
    dpi: i64,
    layer: bool,
    needed_by: &str,
) -> Result<OcrPage> {
    let program = require(ctx, "tesseract", needed_by)?;
    let base = image.with_extension("ocr");
    let mut args = tesseract_args(image, &base.display().to_string(), lang);
    args.extend([
        "--psm".to_string(),
        "1".to_string(),
        "--dpi".to_string(),
        dpi.to_string(),
    ]);
    if layer {
        args.extend(["-c".to_string(), "textonly_pdf=1".to_string()]);
    }
    args.push("txt".to_string());
    if layer {
        args.push("pdf".to_string());
    }
    let out = run(ctx, &program, &args).map_err(|e| tesseract_error(e, lang))?;
    let text = std::fs::read(base.with_extension("ocr.txt"))
        .map(|b| String::from_utf8_lossy(&b).to_string())
        .unwrap_or_default();
    let pdf = base.with_extension("ocr.pdf");
    if layer && std::fs::metadata(&pdf).map_or(true, |m| m.len() == 0) {
        // Without its pdf.ttf in the tessdata folder tesseract writes no PDF
        // and still exits 0.
        let why = tail(&out.stderr, 3);
        return Err(err(
            ErrorCode::EngineFailed,
            format!("tesseract wrote no PDF layer: {why}"),
        )
        .hint("tesseract needs pdf.ttf in its tessdata folder (TESSDATA_PREFIX) to write PDF"));
    }
    Ok(OcrPage {
        text,
        layer: layer.then_some(pdf),
        no_orientation: out.stderr.contains("osd language failed to load")
            || out.stderr.contains("Failed loading language 'osd'"),
    })
}
