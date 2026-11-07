//! Operations (one file each under `src/ops/<group>/`) and the runtime that
//! executes them. See build.rs for how the registry is generated.

pub mod generated {
    include!(concat!(env!("OUT_DIR"), "/registry_gen.rs"));
}
pub mod registry;
pub mod runtime;
pub mod structured;

pub use registry::registry;
