//! External engines: locating, installing and running programs such as ffmpeg
//! and yt-dlp. Engines are never invoked through a shell; arguments are passed
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
    /// Can `deft engine install` fetch it?
    pub installable: bool,
    /// How to install it by hand.
    pub manual: &'static str,
}

const VERSION_TIMEOUT: Duration = Duration::from_secs(5);

pub const ENGINES: &[Engine] = &[
    Engine {
        name: "ffmpeg",
        purpose: "video and audio processing",
        version_arg: "-version",
        installable: false,
        manual: "brew install ffmpeg | apt install ffmpeg | winget install ffmpeg",
    },
    Engine {
        name: "ffprobe",
        purpose: "video and audio inspection (ships with ffmpeg)",
        version_arg: "-version",
        installable: false,
        manual: "brew install ffmpeg | apt install ffmpeg | winget install ffmpeg",
    },
    Engine {
        name: "yt-dlp",
        purpose: "downloading from YouTube and other sites",
        version_arg: "--version",
        installable: true,
        manual: "deft engine install yt-dlp",
    },
    Engine {
        name: "gs",
        purpose: "repairing PDFs, PDF/A and grayscale (Ghostscript)",
        version_arg: "--version",
        installable: false,
        manual: "brew install ghostscript | apt install ghostscript | winget install ghostscript",
    },
    Engine {
        name: "sqlite3",
        purpose: "SQL over tables and SQLite files",
        version_arg: "--version",
        installable: false,
        manual: "brew install sqlite | apt install sqlite3 | winget install sqlite",
    },
    Engine {
        name: "chrome",
        purpose: "exact web page rendering (Chrome, Chromium or Edge)",
        version_arg: "--version",
        installable: false,
        manual: "install Google Chrome or Chromium",
    },
    Engine {
        name: "tesseract",
        purpose: "reading text in images (OCR)",
        version_arg: "--version",
        installable: false,
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
    let on_path = |n: &str| dirs.iter().map(|d| d.join(exe(n))).find(|p| p.is_file());
    if let Some(p) = on_path(name) {
        return Some(p);
    }
    match name {
        "chrome" => browser(&on_path),
        _ => None,
    }
}

fn browser(on_path: &dyn Fn(&str) -> Option<PathBuf>) -> Option<PathBuf> {
    const NAMES: &[&str] = &[
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
        "microsoft-edge",
    ];
    const APPS: &[&str] = &[
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        "C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe",
    ];
    NAMES
        .iter()
        .find_map(|n| on_path(n))
        .or_else(|| APPS.iter().map(PathBuf::from).find(|p| p.is_file()))
}

/// Find an engine or fail with ENGINE_MISSING (installing it first when
/// `--install-engines` is set and that is possible).
pub fn require(ctx: &Ctx, name: &str, needed_by: &str) -> Result<PathBuf> {
    if let Some(p) = locate(ctx, name) {
        return Ok(p);
    }
    let def =
        engine(name).ok_or_else(|| err(ErrorCode::Internal, format!("unknown engine {name}")))?;
    if ctx.install_engines && def.installable {
        return install(ctx, name);
    }
    let hint = if def.installable {
        format!("deft engine install {name}   (one-time download) or rerun with --install-engines")
    } else {
        format!("install it: {}", def.manual)
    };
    Err(err(
        ErrorCode::EngineMissing,
        format!("{needed_by} needs {name} (not found)"),
    )
    .hint(hint))
}

pub fn version(path: &Path, version_arg: &str) -> Option<String> {
    // On Windows a browser opens a window for --version and never exits.
    let name = path.file_stem()?.to_string_lossy().to_lowercase();
    if cfg!(windows) && (name.contains("chrome") || name == "msedge") {
        return None;
    }
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

/// ffmpeg options that make Ogg audio Vorbis. Left to itself, an ffmpeg built
/// without libvorbis silently writes FLAC into the file. `encoders` is the
/// output of `ffmpeg -encoders`.
pub fn vorbis_args(encoders: &str) -> Vec<String> {
    let args: &[&str] = if encoders.contains("libvorbis") {
        &["-c:a", "libvorbis"]
    } else {
        // The built-in encoder is in every build, but only does stereo.
        &["-c:a", "vorbis", "-strict", "experimental", "-ac", "2"]
    };
    args.iter().map(|s| s.to_string()).collect()
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

fn yt_dlp_asset() -> Option<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", _) => Some("yt-dlp_macos"),
        ("linux", "aarch64") => Some("yt-dlp_linux_aarch64"),
        ("linux", _) => Some("yt-dlp_linux"),
        ("windows", _) => Some("yt-dlp.exe"),
        _ => None,
    }
}

fn fetch(url: &str) -> Result<Vec<u8>> {
    let net = |e: String| err(ErrorCode::Network, format!("{url}: {e}"));
    let mut resp = ureq::get(url).call().map_err(|e| net(e.to_string()))?;
    resp.body_mut()
        .with_config()
        .limit(512 * 1024 * 1024)
        .read_to_vec()
        .map_err(|e| net(e.to_string()))
}

fn sha256_hex(data: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(data)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Download an engine into the managed directory, verifying its checksum.
pub fn install(ctx: &Ctx, name: &str) -> Result<PathBuf> {
    if ctx.offline {
        return Err(err(
            ErrorCode::Offline,
            format!("installing {name} needs the network, but --offline is set"),
        ));
    }
    let def =
        engine(name).ok_or_else(|| err(ErrorCode::BadValue, format!("unknown engine {name}")))?;
    if !def.installable {
        return Err(err(
            ErrorCode::EngineMissing,
            format!("{name} cannot be installed by deft yet"),
        )
        .hint(format!("install it: {}", def.manual)));
    }
    let asset = yt_dlp_asset().ok_or_else(|| {
        err(
            ErrorCode::EngineMissing,
            "no yt-dlp build for this platform",
        )
    })?;
    let base = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";
    let sums = String::from_utf8_lossy(&fetch(&format!("{base}/SHA2-256SUMS"))?).to_string();
    let expected = sums
        .lines()
        .find_map(|l| {
            l.split_once(char::is_whitespace)
                .filter(|(_, f)| f.trim().trim_start_matches('*') == asset)
                .map(|(h, _)| h.to_string())
        })
        .ok_or_else(|| {
            err(
                ErrorCode::Network,
                format!("no checksum published for {asset}"),
            )
        })?;
    let data = fetch(&format!("{base}/{asset}"))?;
    let actual = sha256_hex(&data);
    if actual != expected {
        return Err(err(
            ErrorCode::Network,
            format!("checksum mismatch for {asset}: expected {expected}, got {actual}"),
        ));
    }
    std::fs::create_dir_all(&ctx.engine_dir).map_err(|e| {
        err(
            ErrorCode::OutputUnwritable,
            format!("{}: {e}", ctx.engine_dir.display()),
        )
    })?;
    let dest = ctx.engine_dir.join(exe(name));
    let tmp = dest.with_extension("download");
    std::fs::write(&tmp, &data).map_err(|e| err(ErrorCode::OutputUnwritable, e.to_string()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| err(ErrorCode::OutputUnwritable, e.to_string()))?;
    }
    std::fs::rename(&tmp, &dest).map_err(|e| err(ErrorCode::OutputUnwritable, e.to_string()))?;
    Ok(dest)
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

/// Explain the common failure: a language pack that is not installed.
fn tesseract_error(mut e: deft_core::error::DeftError, lang: &str) -> deft_core::error::DeftError {
    if e.message.contains("Failed loading language") || e.message.contains("Error opening data") {
        e.hint = Some(format!(
            "language data for {lang} is missing: brew install tesseract-lang | apt install tesseract-ocr-{lang}"
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

/// A PDF page holding only the invisible text of one image. Returns its path.
pub fn ocr_layer(
    ctx: &Ctx,
    image: &Path,
    lang: &str,
    dpi: i64,
    needed_by: &str,
) -> Result<PathBuf> {
    let program = require(ctx, "tesseract", needed_by)?;
    let base = image.with_extension("layer");
    let mut args = tesseract_args(image, &base.display().to_string(), lang);
    args.extend([
        "--dpi".to_string(),
        dpi.to_string(),
        "-c".to_string(),
        "textonly_pdf=1".to_string(),
        "pdf".to_string(),
    ]);
    run(ctx, &program, &args).map_err(|e| tesseract_error(e, lang))?;
    Ok(base.with_extension("layer.pdf"))
}

/// Run a browser that writes `output`, with a time limit. A page can stop it
/// for ever, for example with a script that opens a dialog nobody can answer.
/// Chrome can also crash while shutting down, after the file is written, so
/// the file decides whether the job worked, not the exit code.
pub fn run_browser(ctx: &Ctx, program: &Path, args: &[String], output: &Path) -> Result<()> {
    let mut limited = ctx.clone();
    limited.timeout = ctx.timeout.or(Some(Duration::from_secs(60)));
    match run(&limited, program, args) {
        Err(e) if e.code == ErrorCode::EngineFailed => {
            let written = std::fs::metadata(output).is_ok_and(|m| m.len() > 0);
            if written { Ok(()) } else { Err(e) }
        }
        other => other.map(|_| ()),
    }
}

/// Headless browser arguments. The network is blocked: these commands work on
/// local files, and deft only touches the network in `network` operations.
pub fn browser_args(profile: &Path) -> Vec<String> {
    [
        "--headless=new",
        "--disable-gpu",
        "--no-first-run",
        "--no-default-browser-check",
        "--hide-scrollbars",
        "--host-resolver-rules=MAP * ~NOTFOUND",
    ]
    .iter()
    .map(|s| s.to_string())
    .chain([format!("--user-data-dir={}", profile.display())])
    .collect()
}
