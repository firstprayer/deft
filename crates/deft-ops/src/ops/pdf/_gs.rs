//! PDF jobs that go through Ghostscript.

use deft_core::prelude::*;
use std::path::Path;

/// Rewrite a PDF with `pdfwrite` and extra options.
pub fn rewrite(job: &Job, id: &str, source: &Path, extra: &[&str]) -> Result<OpResult> {
    let program = deft_engines::require(job.ctx, "gs", id)?;
    let out = job.ctx.temp_path("out.pdf");
    let mut args: Vec<String> = ["-q", "-dNOPAUSE", "-dBATCH", "-dSAFER", "-sDEVICE=pdfwrite"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    args.extend(extra.iter().map(|s| s.to_string()));
    args.push(format!("-sOutputFile={}", out.display()));
    args.push(source.display().to_string());
    deft_engines::run(job.ctx, &program, &args)?;
    Ok(OpResult::one(Produced::file(out, Some(Format::Pdf))))
}

/// The input without a password, in a scratch file Ghostscript can read.
pub fn plain_copy(job: &Job) -> Result<std::path::PathBuf> {
    let mut doc = super::_common::open(job.input(), job.params)?;
    let path = job.ctx.temp_path("plain.pdf");
    std::fs::write(&path, deft_tools_pdf::save(&mut doc)?)?;
    Ok(path)
}
