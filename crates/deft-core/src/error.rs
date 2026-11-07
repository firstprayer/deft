//! Stable error codes. Codes are part of the public contract: add, never rename.

use std::fmt;

macro_rules! codes {
    ($($v:ident => $s:literal, $exit:literal, $doc:literal;)*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum ErrorCode { $($v,)* }
        impl ErrorCode {
            pub const ALL: &'static [ErrorCode] = &[$(ErrorCode::$v,)*];
            pub fn as_str(self) -> &'static str { match self { $(ErrorCode::$v => $s,)* } }
            /// Process exit code for this error class.
            pub fn exit_code(self) -> i32 { match self { $(ErrorCode::$v => $exit,)* } }
            pub fn doc(self) -> &'static str { match self { $(ErrorCode::$v => $doc,)* } }
        }
    };
}

codes! {
    UnknownCommand => "UNKNOWN_COMMAND", 2, "No such group or command; see the suggestions";
    BadOption => "BAD_OPTION", 2, "Unknown or misplaced option";
    BadValue => "BAD_VALUE", 2, "An option value does not match its syntax";
    InputNotFound => "INPUT_NOT_FOUND", 3, "An input path does not exist";
    InputUnsupported => "INPUT_UNSUPPORTED", 3, "The input format is not supported by this command";
    InputMismatch => "INPUT_MISMATCH", 3, "The input is a different format than the command expects";
    InputCorrupt => "INPUT_CORRUPT", 3, "The input could not be parsed";
    InputEncrypted => "INPUT_ENCRYPTED", 3, "The input is password-protected";
    OutputExists => "OUTPUT_EXISTS", 1, "The output already exists; pass --force to overwrite";
    OutputUnwritable => "OUTPUT_UNWRITABLE", 1, "The output location cannot be written";
    EngineMissing => "ENGINE_MISSING", 4, "An external engine is required; the hint shows how to install it";
    EngineFailed => "ENGINE_FAILED", 1, "An external engine reported an error";
    NoConversionPath => "NO_CONVERSION_PATH", 3, "No chain of converters reaches the requested format";
    TargetUnreachable => "TARGET_UNREACHABLE", 1, "The requested target (such as a file size) cannot be reached";
    LimitExceeded => "LIMIT_EXCEEDED", 3, "The input exceeds a safety limit (pixels, pages, size)";
    Timeout => "TIMEOUT", 1, "The operation exceeded --timeout";
    Internal => "INTERNAL", 1, "A bug in deft; please report it";
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An error with a stable code, a one-line message and an optional
/// copy-pasteable hint.
#[derive(Debug, Clone)]
pub struct DeftError {
    pub code: ErrorCode,
    pub message: String,
    pub hint: Option<String>,
}

impl DeftError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        DeftError {
            code,
            message: message.into(),
            hint: None,
        }
    }
    pub fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

impl fmt::Display for DeftError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.code, self.message)
    }
}

impl std::error::Error for DeftError {}

impl From<std::io::Error> for DeftError {
    fn from(e: std::io::Error) -> Self {
        let code = match e.kind() {
            std::io::ErrorKind::NotFound => ErrorCode::InputNotFound,
            std::io::ErrorKind::PermissionDenied => ErrorCode::OutputUnwritable,
            _ => ErrorCode::Internal,
        };
        DeftError::new(code, e.to_string())
    }
}

pub type Result<T, E = DeftError> = std::result::Result<T, E>;

/// Shorthand constructors.
pub fn err(code: ErrorCode, message: impl Into<String>) -> DeftError {
    DeftError::new(code, message)
}

pub fn bad_value(option: &str, message: impl fmt::Display) -> DeftError {
    DeftError::new(ErrorCode::BadValue, format!("--{option}: {message}"))
}

pub fn corrupt(what: impl fmt::Display, detail: impl fmt::Display) -> DeftError {
    DeftError::new(ErrorCode::InputCorrupt, format!("{what}: {detail}"))
}
