//! Helpers shared by the youtube operations (all built on yt-dlp).

use deft_core::prelude::*;
use std::path::PathBuf;

pub const ENGINES: &[&str] = &["yt-dlp"];
pub const COOKIES: ParamSpec = param(
    "cookies-from-browser",
    Kind::Text,
    "Use your browser's cookies: chrome, firefox, safari",
);

pub fn yt_dlp(job: &Job, id: &str, mut args: Vec<String>) -> Result<Vec<u8>> {
    let path = deft_engines::require(job.ctx, "yt-dlp", id)?;
    if let Some(b) = job.params.opt_text("cookies-from-browser") {
        args.extend(["--cookies-from-browser".to_string(), b]);
    }
    if let Some(ffmpeg) = deft_engines::locate(job.ctx, "ffmpeg") {
        args.extend([
            "--ffmpeg-location".to_string(),
            ffmpeg.display().to_string(),
        ]);
    }
    args.extend(["--no-warnings".to_string(), "--no-progress".to_string()]);
    args.push("--".to_string());
    args.push(job.input().url.clone().unwrap_or_default());
    Ok(deft_engines::run(job.ctx, &path, &args)
        .map_err(friendly)?
        .stdout)
}

/// Turn common yt-dlp failures into something an agent can act on.
fn friendly(mut e: DeftError) -> DeftError {
    let m = e.message.to_ascii_lowercase();
    if m.contains("sign in")
        || m.contains("login")
        || m.contains("private video")
        || m.contains("confirm you")
    {
        e = e.hint("this video needs a signed-in session; add --cookies-from-browser chrome");
    } else if m.contains("unavailable") || m.contains("not available") {
        e = e.hint("the video may be removed or blocked in this region");
    } else {
        e = e.hint("YouTube changes often; try: deft engine update yt-dlp");
    }
    e
}

/// First file in a fresh download directory (optionally by extension).
pub fn only_file(dir: &std::path::Path, ext: Option<&str>) -> Option<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    files.sort();
    files
        .into_iter()
        .find(|p| ext.is_none_or(|x| p.extension().is_some_and(|e| e == x)))
}

pub fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

pub fn template(dir: &std::path::Path) -> String {
    dir.join("%(title).100B [%(id)s].%(ext)s")
        .display()
        .to_string()
}

pub fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "download".into())
}

pub fn fresh_dir(job: &Job) -> Result<PathBuf> {
    let dir = job.ctx.temp_path("download");
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}
