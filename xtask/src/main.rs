//! Developer tools: `cargo xtask <review|accept|bench|eval|catalog|...>`.

mod bench;
mod catalog;
mod commands;
mod eval;
mod notices;
mod review;
mod site;

use std::path::PathBuf;

/// Repository root.
pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rest = args.get(1..).unwrap_or_default();
    let result = match args.first().map(String::as_str) {
        Some("review") => review::review(),
        Some("accept") => review::accept(rest),
        Some("bench") => bench::run(),
        Some("eval") => eval::run(rest),
        Some("notices") => notices::write(),
        Some("commands") => {
            commands::print();
            Ok(())
        }
        Some("catalog") => catalog::write(rest.iter().any(|a| a == "--open")),
        Some("site") => site::write(rest.iter().any(|a| a == "--check")),
        Some("page") => match rest {
            [template, output] => site::write_page(template, output),
            _ => Err("usage: cargo xtask page <template> <output>".to_string()),
        },
        Some("setup") => setup(),
        Some("check") => check(),
        Some("fmt") => fmt(rest.iter().any(|a| a == "--check")),
        _ => Err("usage: cargo xtask review | accept <case>... | accept --new | bench | notices | commands | catalog [--open] | site [--check] | setup | check | fmt [--check] | eval <fetch|lint|list|run|report>".to_string()),
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

/// Everything a change must pass. The pre-push hook runs this.
fn check() -> Result<(), String> {
    fmt(true)?;
    let steps: &[&[&str]] = &[
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
        &["test-unit"],
        &["test-e2e"],
        &["xtask", "eval", "lint"],
        &["xtask", "site", "--check"],
    ];
    for args in steps {
        let ok = std::process::Command::new("cargo")
            .args(*args)
            .current_dir(root())
            .status()
            .map_err(|e| e.to_string())?
            .success();
        if !ok {
            return Err(format!("cargo {} failed", args.join(" ")));
        }
    }
    println!("all checks passed");
    Ok(())
}

/// Format everything. Operation files are compiled through a generated module
/// list that `cargo fmt` cannot follow, so they are passed to rustfmt directly.
fn fmt(check: bool) -> Result<(), String> {
    use std::process::Command;
    let mut cargo = Command::new("cargo");
    cargo.args(["fmt", "--all"]).current_dir(root());
    let pattern = root().join("crates/deft-ops/src/ops/**/*.rs");
    let files: Vec<PathBuf> = glob::glob(&pattern.display().to_string())
        .map_err(|e| e.to_string())?
        .flatten()
        .collect();
    let mut rustfmt = Command::new("rustfmt");
    rustfmt.args(["--edition", "2024"]).args(&files);
    if check {
        cargo.arg("--check");
        rustfmt.arg("--check");
    }
    let ok = [cargo, rustfmt]
        .iter_mut()
        .map(|c| c.status().map_err(|e| e.to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    if ok.iter().all(|s| s.success()) {
        Ok(())
    } else {
        Err("not formatted; run: cargo xtask fmt".into())
    }
}

/// One-time setup for a clone: use the repository's git hooks.
fn setup() -> Result<(), String> {
    let ok = std::process::Command::new("git")
        .args(["config", "core.hooksPath", ".githooks"])
        .current_dir(root())
        .status()
        .map_err(|e| e.to_string())?
        .success();
    if !ok {
        return Err("git config failed".into());
    }
    println!("git hooks enabled");
    Ok(())
}

/// Build the deft binary and return its path.
pub fn build_deft(release: bool) -> Result<PathBuf, String> {
    let mut c = std::process::Command::new("cargo");
    c.args(["build", "--quiet", "-p", "deftcli"])
        .current_dir(root());
    if release {
        c.arg("--release");
    }
    let ok = c.status().map_err(|e| e.to_string())?.success();
    if !ok {
        return Err("cargo build failed".into());
    }
    Ok(root()
        .join("target")
        .join(if release { "release" } else { "debug" })
        .join("deft"))
}
