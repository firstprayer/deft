//! Shared types for Deft: formats, option values, errors, operation specs and
//! the job/result types. Capability libraries and operations depend on this
//! crate; it depends on nothing Deft-specific.

pub mod error;
pub mod format;
pub mod job;
pub mod spec;
pub mod value;

pub mod prelude {
    pub use crate::error::{DeftError, ErrorCode, Result, bad_value, corrupt, err};
    pub use crate::facts;
    pub use crate::format::Format;
    pub use crate::job::{Context, Ctx, Data, InputFile, Job, OpResult, Params, Produced};
    pub use crate::spec::{
        Accept, Edge, Group, Input, InputArity, Op, Output, OutputArity, ParamSpec, Quality,
        Stability, param,
    };
    pub use crate::value::{
        Kind, PageSet, Position, SizeSpec, Value, format_bytes, format_duration, percent_decode,
        right_angle,
    };
}
