//! Helpers shared by the file operations.

use deft_core::prelude::*;
use std::path::PathBuf;

/// Inputs as paths, with directories expanded.
pub fn files(job: &Job) -> Result<Vec<PathBuf>> {
    let paths: Vec<PathBuf> = job.inputs.iter().filter_map(|i| i.path.clone()).collect();
    deft_tools_file::walk(&paths)
}

/// Bytes of unknown type: written to a scratch file so that any `-o` name fits.
pub fn raw_output(job: &Job, bytes: &[u8], name: String) -> Result<Produced> {
    let path = job.ctx.temp_path("out.bin");
    std::fs::write(&path, bytes)?;
    Ok(Produced::file(path, None).named(name))
}
