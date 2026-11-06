//! RAR through bsdtar. Everything else is built in.

use deft_core::prelude::*;
use std::path::{Path, PathBuf};

/// Old libarchive builds fail on many RAR files, and conda and Homebrew can
/// shadow a newer system copy, so that copy gets a second chance.
fn bsdtar(job: &Job, args: Vec<String>) -> Result<String> {
    let program = deft_engines::require(job.ctx, "bsdtar", "reading RAR")?;
    let system = Path::new("/usr/bin/bsdtar");
    let out = deft_engines::run(job.ctx, &program, &args).or_else(|e| {
        if system.is_file() && program != system {
            deft_engines::run(job.ctx, system, &args).map_err(|_| e)
        } else {
            Err(e)
        }
    })?;
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

pub fn list_rar(job: &Job, path: &Path) -> Result<Vec<(String, u64)>> {
    let text = bsdtar(job, vec!["-tf".to_string(), path.display().to_string()])?;
    let names: Vec<String> = text
        .lines()
        .filter(|l| !l.ends_with('/'))
        .map(String::from)
        .collect();
    // Sizes come from a verbose listing, whose format varies; names are what matter.
    Ok(names.into_iter().map(|n| (n, 0)).collect())
}

/// bsdtar refuses `..` and absolute paths unless told otherwise.
pub fn extract_rar(job: &Job, path: &Path, dir: PathBuf) -> Result<OpResult> {
    bsdtar(
        job,
        vec![
            "-xf".to_string(),
            path.display().to_string(),
            "-C".to_string(),
            dir.display().to_string(),
        ],
    )?;
    let files = deft_tools_file::walk(std::slice::from_ref(&dir))?.len();
    Ok(OpResult::one(Produced::dir(dir))
        .fact("files", files)
        .note(format!("{files} file{}", if files == 1 { "" } else { "s" })))
}
