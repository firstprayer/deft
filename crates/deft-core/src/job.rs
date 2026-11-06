//! What an operation receives (a Job) and returns (an OpResult).

use crate::error::{DeftError, ErrorCode, Result, err};
use crate::format::Format;
use crate::value::{PageSet, Position, SizeSpec, Value};
use serde_json::{Map, Value as Json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

/// Safety limits applied to untrusted input.
#[derive(Clone, Debug)]
pub struct Limits {
    pub max_pixels: u64,
    pub max_pages: usize,
    pub max_unpacked_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_pixels: 200_000_000,
            max_pages: 10_000,
            max_unpacked_bytes: 8 * 1024 * 1024 * 1024,
        }
    }
}

/// Settings shared by every job in one invocation.
#[derive(Clone, Debug)]
pub struct Ctx {
    /// Scratch directory, removed when the invocation ends.
    pub tmp: PathBuf,
    pub limits: Limits,
    pub offline: bool,
    pub install_engines: bool,
    pub keep_metadata: bool,
    pub timeout: Option<Duration>,
    /// Directory where managed engines are installed.
    pub engine_dir: PathBuf,
}

/// The directory deft keeps its own files in.
pub fn deft_home() -> PathBuf {
    std::env::var_os("DEFT_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .or_else(|| std::env::var_os("USERPROFILE"))
                .map(|h| PathBuf::from(h).join(".deft"))
        })
        .unwrap_or_else(|| PathBuf::from(".deft"))
}

impl Ctx {
    pub fn new(tmp: PathBuf) -> Ctx {
        Ctx {
            tmp,
            limits: Limits::default(),
            offline: std::env::var_os("DEFT_OFFLINE").is_some_and(|v| v != "0"),
            install_engines: std::env::var_os("DEFT_AUTO_INSTALL").is_some_and(|v| v != "0"),
            keep_metadata: false,
            timeout: None,
            engine_dir: deft_home().join("engines"),
        }
    }

    /// A fresh path inside the scratch directory.
    pub fn temp_path(&self, name: &str) -> PathBuf {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = self.tmp.join(format!("t{n}"));
        let _ = std::fs::create_dir_all(&dir);
        dir.join(name)
    }
}

/// One input of a job: a local file, stdin bytes or a URL.
#[derive(Debug)]
pub struct InputFile {
    /// What the user typed, for messages.
    pub display: String,
    pub path: Option<PathBuf>,
    pub url: Option<String>,
    pub format: Option<Format>,
    bytes: OnceLock<Vec<u8>>,
}

impl InputFile {
    pub fn file(path: PathBuf, format: Option<Format>) -> InputFile {
        InputFile {
            display: path.display().to_string(),
            path: Some(path),
            url: None,
            format,
            bytes: OnceLock::new(),
        }
    }
    pub fn stdin(bytes: Vec<u8>, format: Option<Format>) -> InputFile {
        let lock = OnceLock::new();
        let _ = lock.set(bytes);
        InputFile {
            display: "-".into(),
            path: None,
            url: None,
            format,
            bytes: lock,
        }
    }
    pub fn url(url: String) -> InputFile {
        InputFile {
            display: url.clone(),
            path: None,
            url: Some(url),
            format: None,
            bytes: OnceLock::new(),
        }
    }

    /// File contents (read once, then cached).
    pub fn bytes(&self) -> Result<&[u8]> {
        if let Some(b) = self.bytes.get() {
            return Ok(b);
        }
        let path = self
            .path
            .as_ref()
            .ok_or_else(|| err(ErrorCode::Internal, "input has no bytes"))?;
        let data = std::fs::read(path)
            .map_err(|e| err(ErrorCode::InputNotFound, format!("{}: {e}", path.display())))?;
        Ok(self.bytes.get_or_init(|| data))
    }

    /// Path on disk, materialising stdin input into a temp file when needed
    /// (external engines need real files).
    pub fn local_path(&self, ctx: &Ctx) -> Result<PathBuf> {
        if let Some(p) = &self.path {
            return Ok(p.clone());
        }
        let ext = self.format.map(|f| f.ext()).unwrap_or("bin");
        let p = ctx.temp_path(&format!("stdin.{ext}"));
        std::fs::write(&p, self.bytes()?)?;
        Ok(p)
    }

    /// File name without extension, used to name outputs.
    pub fn stem(&self) -> String {
        match &self.path {
            Some(p) => {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "output".into());
                let lower = name.to_ascii_lowercase();
                for double in [
                    ".tar.gz", ".tgz", ".tar.bz2", ".tbz2", ".tar.xz", ".txz", ".tar.zst", ".tzst",
                ] {
                    if lower.ends_with(double) {
                        return name[..name.len() - double.len()].to_string();
                    }
                }
                match name.rfind('.') {
                    Some(i) if i > 0 => name[..i].to_string(),
                    _ => name,
                }
            }
            None => "output".into(),
        }
    }

    pub fn require_format(&self) -> Result<Format> {
        self.format.ok_or_else(|| {
            err(
                ErrorCode::InputUnsupported,
                format!("{}: unknown file type", self.display),
            )
        })
    }
}

/// Parsed option values for one invocation (defaults filled in).
#[derive(Clone, Debug, Default)]
pub struct Params {
    values: BTreeMap<String, Value>,
}

macro_rules! getter {
    ($name:ident, $opt:ident, $variant:ident, $ty:ty) => {
        pub fn $opt(&self, key: &str) -> Option<$ty> {
            match self.values.get(key) {
                Some(Value::$variant(v)) => Some(v.clone()),
                None => None,
                Some(other) => panic!("option --{key} is {other:?}, not {}", stringify!($variant)),
            }
        }
        pub fn $name(&self, key: &str) -> $ty {
            self.$opt(key)
                .unwrap_or_else(|| panic!("option --{key} has no value and no default"))
        }
    };
}

impl Params {
    pub fn set(&mut self, key: &str, value: Value) {
        self.values.insert(key.to_string(), value);
    }
    pub fn has(&self, key: &str) -> bool {
        self.values.contains_key(key)
    }
    pub fn remove(&mut self, key: &str) {
        self.values.remove(key);
    }
    pub fn flag(&self, key: &str) -> bool {
        matches!(self.values.get(key), Some(Value::Flag(true)))
    }
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Value)> {
        self.values.iter()
    }
    getter!(int, opt_int, Int, i64);
    getter!(float, opt_float, Float, f64);
    getter!(text, opt_text, Text, String);
    getter!(pages, opt_pages, Pages, PageSet);
    getter!(seconds, opt_seconds, Seconds, f64);
    getter!(times, opt_times, Times, Vec<f64>);
    getter!(size, opt_size, Size, SizeSpec);
    getter!(points, opt_points, Points, f64);
    getter!(bytes, opt_bytes, Bytes, u64);
    getter!(color, opt_color, Color, [u8; 4]);
    getter!(position, opt_position, Position, Position);
    getter!(path, opt_path, Path, PathBuf);

    pub fn to_json(&self) -> Json {
        Json::Object(
            self.values
                .iter()
                .map(|(k, v)| (k.clone(), v.to_json()))
                .collect(),
        )
    }
}

/// Everything an operation needs to run once.
pub struct Job<'a> {
    pub inputs: Vec<InputFile>,
    pub params: &'a Params,
    pub ctx: &'a Ctx,
    /// Output format requested through `-o name.ext`, when the operation can
    /// write that format.
    pub out_format: Option<Format>,
}

impl Job<'_> {
    pub fn input(&self) -> &InputFile {
        &self.inputs[0]
    }

    /// The format to write: what `-o name.ext` asked for, else `default`.
    pub fn target(&self, default: Format) -> Format {
        self.out_format.unwrap_or(default)
    }
}

/// Produced data.
#[derive(Debug)]
pub enum Data {
    Bytes(Vec<u8>),
    /// A finished file in the scratch directory, moved into place.
    File(PathBuf),
    /// A finished directory in the scratch directory, moved into place.
    Dir(PathBuf),
}

/// One output of a job.
#[derive(Debug)]
pub struct Produced {
    pub data: Data,
    pub format: Option<Format>,
    /// Suggested file name (without directory). For multi-output jobs this
    /// usually carries an index, e.g. "report-003.png".
    pub name: Option<String>,
    pub facts: Map<String, Json>,
}

impl Produced {
    pub fn bytes(data: Vec<u8>, format: Format) -> Produced {
        Produced {
            data: Data::Bytes(data),
            format: Some(format),
            name: None,
            facts: Map::new(),
        }
    }
    pub fn file(path: PathBuf, format: Option<Format>) -> Produced {
        Produced {
            data: Data::File(path),
            format,
            name: None,
            facts: Map::new(),
        }
    }
    pub fn dir(path: PathBuf) -> Produced {
        Produced {
            data: Data::Dir(path),
            format: None,
            name: None,
            facts: Map::new(),
        }
    }
    pub fn named(mut self, name: impl Into<String>) -> Produced {
        self.name = Some(name.into());
        self
    }
    pub fn fact(mut self, key: &str, value: impl Into<Json>) -> Produced {
        self.facts.insert(key.into(), value.into());
        self
    }
    pub fn len(&self) -> Option<u64> {
        match &self.data {
            Data::Bytes(b) => Some(b.len() as u64),
            Data::File(p) => std::fs::metadata(p).ok().map(|m| m.len()),
            Data::Dir(_) => None,
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == Some(0)
    }
}

/// What a job returns.
#[derive(Debug, Default)]
pub struct OpResult {
    pub outputs: Vec<Produced>,
    pub facts: Map<String, Json>,
    /// Short human summary appended to the result line, e.g. "3 pages".
    pub note: String,
    pub warnings: Vec<String>,
}

impl OpResult {
    pub fn one(p: Produced) -> OpResult {
        OpResult {
            outputs: vec![p],
            ..Default::default()
        }
    }
    pub fn many(outputs: Vec<Produced>) -> OpResult {
        OpResult {
            outputs,
            ..Default::default()
        }
    }
    pub fn report(facts: Map<String, Json>, note: impl Into<String>) -> OpResult {
        OpResult {
            outputs: vec![],
            facts,
            note: note.into(),
            warnings: vec![],
        }
    }
    pub fn note(mut self, note: impl Into<String>) -> OpResult {
        self.note = note.into();
        self
    }
    pub fn fact(mut self, key: &str, value: impl Into<Json>) -> OpResult {
        self.facts.insert(key.into(), value.into());
        self
    }
    pub fn warn(mut self, w: impl Into<String>) -> OpResult {
        self.warnings.push(w.into());
        self
    }
}

/// Build a JSON object from pairs: `facts![("pages", 3), ("title", "x")]`.
#[macro_export]
macro_rules! facts {
    ($(($k:expr, $v:expr)),* $(,)?) => {{
        let mut m = serde_json::Map::new();
        $( m.insert($k.to_string(), serde_json::json!($v)); )*
        m
    }};
}

/// Convenience for operations: map any displayable error to INPUT_CORRUPT
/// with the input name.
pub trait Context<T> {
    fn ctx(self, what: &str) -> Result<T>;
}

impl<T, E: std::fmt::Display> Context<T> for std::result::Result<T, E> {
    fn ctx(self, what: &str) -> Result<T> {
        self.map_err(|e| DeftError::new(ErrorCode::InputCorrupt, format!("{what}: {e}")))
    }
}

pub fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default()
}
