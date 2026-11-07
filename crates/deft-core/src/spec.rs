//! Static descriptions of groups and operations. Help, `find`, JSON Schema,
//! the skill file and the contract tests are all generated from these.

use crate::error::{ErrorCode, Result};
use crate::format::Format;
use crate::job::{Job, OpResult};
use crate::value::Kind;

/// A logical group of commands. Groups are only an index: they can be organised
/// by file type, such as pdf.
#[derive(Debug)]
pub struct Group {
    pub name: &'static str,
    pub summary: &'static str,
    /// Alternative group names that resolve silently, e.g. `img` -> `image`.
    pub aliases: &'static [&'static str],
    /// Verb synonyms within the group, e.g. ("combine", "merge").
    pub synonyms: &'static [(&'static str, &'static str)],
    /// Sort key for `deft` help (lower first).
    pub order: u32,
}

/// One option of an operation.
#[derive(Clone, Copy, Debug)]
pub struct ParamSpec {
    pub name: &'static str,
    pub kind: Kind,
    pub default: Option<&'static str>,
    pub help: &'static str,
    pub required: bool,
}

impl ParamSpec {
    pub const fn new(name: &'static str, kind: Kind, help: &'static str) -> ParamSpec {
        ParamSpec {
            name,
            kind,
            default: None,
            help,
            required: false,
        }
    }
    pub const fn default(mut self, value: &'static str) -> ParamSpec {
        self.default = Some(value);
        self
    }
    pub const fn required(mut self) -> ParamSpec {
        self.required = true;
        self
    }
}

/// Shorthand: `param("level", Kind::Choice(&[..]), "help")`.
pub const fn param(name: &'static str, kind: Kind, help: &'static str) -> ParamSpec {
    ParamSpec::new(name, kind, help)
}

/// What an operation accepts as input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Accept {
    Formats(&'static [Format]),
    /// Any existing file or directory.
    AnyFile,
    /// A web address.
    Url,
    /// Nothing: the operation creates something from options alone.
    Nothing,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum InputArity {
    /// One or more inputs, each processed as its own job.
    Each,
    /// All inputs go to one job (merge, create archive). `min` inputs at least.
    All { min: usize },
    /// No inputs.
    None,
}

#[derive(Clone, Copy, Debug)]
pub struct Input {
    pub accept: Accept,
    pub arity: InputArity,
    /// Shown in help signatures, e.g. "pdf" or "image".
    pub label: &'static str,
}

impl Input {
    pub const fn each(formats: &'static [Format], label: &'static str) -> Input {
        Input {
            accept: Accept::Formats(formats),
            arity: InputArity::Each,
            label,
        }
    }
    pub const fn all(formats: &'static [Format], min: usize, label: &'static str) -> Input {
        Input {
            accept: Accept::Formats(formats),
            arity: InputArity::All { min },
            label,
        }
    }
    pub const fn url() -> Input {
        Input {
            accept: Accept::Url,
            arity: InputArity::Each,
            label: "url",
        }
    }
    pub const fn any_files(min: usize) -> Input {
        Input {
            accept: Accept::AnyFile,
            arity: InputArity::All { min },
            label: "file",
        }
    }
    pub fn accepts(&self, format: Format) -> bool {
        match self.accept {
            Accept::Formats(f) => f.contains(&format),
            Accept::AnyFile => true,
            Accept::Url | Accept::Nothing => false,
        }
    }
    /// Help signature fragment, e.g. "pdf..." or "pdf".
    pub fn signature(&self) -> String {
        match self.arity {
            InputArity::None => "-".into(),
            InputArity::All { min } if min > 1 => format!("{}...", self.label),
            InputArity::All { .. } => format!("{}...", self.label),
            InputArity::Each => self.label.to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OutputArity {
    /// Exactly one file per job.
    One,
    /// Several files per job, written into a directory.
    Many,
    /// A directory (e.g. an extracted archive).
    Dir,
    /// No file: the result is the report itself (info, list).
    Report,
}

#[derive(Clone, Copy, Debug)]
pub struct Output {
    pub arity: OutputArity,
    pub label: &'static str,
    /// Formats the operation may write. Empty for reports and directories.
    pub formats: &'static [Format],
}

impl Output {
    pub const fn one(formats: &'static [Format], label: &'static str) -> Output {
        Output {
            arity: OutputArity::One,
            label,
            formats,
        }
    }
    pub const fn many(formats: &'static [Format], label: &'static str) -> Output {
        Output {
            arity: OutputArity::Many,
            label,
            formats,
        }
    }
    pub const fn report() -> Output {
        Output {
            arity: OutputArity::Report,
            label: "report",
            formats: &[],
        }
    }
    pub const fn dir() -> Output {
        Output {
            arity: OutputArity::Dir,
            label: "dir",
            formats: &[],
        }
    }
    pub fn signature(&self) -> String {
        match self.arity {
            OutputArity::Many => format!("{}...", self.label),
            _ => self.label.to_string(),
        }
    }
}

/// How faithful a conversion is. Used to rank paths in `deft convert`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Quality {
    Lossless,
    Faithful,
    Approximate,
}

impl Quality {
    pub fn cost(self) -> u32 {
        match self {
            Quality::Lossless => 1,
            Quality::Faithful => 3,
            Quality::Approximate => 10,
        }
    }
}

/// A conversion edge: any of `from` can become any of `to`.
#[derive(Clone, Copy, Debug)]
pub struct Edge {
    pub from: &'static [Format],
    pub to: &'static [Format],
    pub quality: Quality,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stability {
    Stable,
    Beta,
}

pub type RunFn = fn(&Job) -> Result<OpResult>;

/// One operation: the unit of implementation. It has one canonical id
/// (`group.command`) and may be mounted at several command paths.
#[derive(Debug)]
pub struct Op {
    pub id: &'static str,
    pub summary: &'static str,
    /// Extra help lines printed after Usage (keep short).
    pub details: &'static [&'static str],
    pub input: Input,
    pub output: Output,
    pub params: &'static [ParamSpec],
    /// Additional command paths, e.g. "doc from-pdf". The canonical path comes from `id`.
    pub mounts: &'static [&'static str],
    pub converts: &'static [Edge],
    pub engines: &'static [&'static str],
    pub examples: &'static [&'static str],
    /// Search keywords and synonyms for `deft find`.
    pub tags: &'static [&'static str],
    /// Names of result facts, documented in help.
    pub result: &'static [&'static str],
    /// Error codes this operation commonly returns, documented in help.
    pub errors: &'static [ErrorCode],
    pub stability: Stability,
    /// Default output name suffix: `report.pdf` -> `report.<suffix>.pdf`.
    /// `None` keeps the stem and only changes the extension.
    pub suffix: Option<&'static str>,
    pub run: RunFn,
}

fn not_implemented(_: &Job) -> Result<OpResult> {
    Err(crate::error::err(
        ErrorCode::Internal,
        "operation has no run function",
    ))
}

impl Op {
    pub const DEFAULT: Op = Op {
        id: "",
        summary: "",
        details: &[],
        input: Input {
            accept: Accept::Nothing,
            arity: InputArity::None,
            label: "-",
        },
        output: Output::report(),
        params: &[],
        mounts: &[],
        converts: &[],
        engines: &[],
        examples: &[],
        tags: &[],
        result: &[],
        errors: &[],
        stability: Stability::Stable,
        suffix: None,
        run: not_implemented,
    };

    pub fn group(&self) -> &'static str {
        self.id.split_once('.').map(|(g, _)| g).unwrap_or(self.id)
    }
    pub fn command(&self) -> &'static str {
        self.id.split_once('.').map(|(_, c)| c).unwrap_or("")
    }
    /// Canonical path, e.g. "pdf merge".
    pub fn path(&self) -> String {
        format!("{} {}", self.group(), self.command())
    }
    /// Every path this operation can be reached by, canonical first.
    pub fn paths(&self) -> Vec<String> {
        let mut v = vec![self.path()];
        v.extend(self.mounts.iter().map(|m| m.to_string()));
        v
    }
    pub fn param(&self, name: &str) -> Option<&ParamSpec> {
        self.params.iter().find(|p| p.name == name)
    }
    pub fn signature(&self) -> String {
        format!("{} -> {}", self.input.signature(), self.output.signature())
    }
}
