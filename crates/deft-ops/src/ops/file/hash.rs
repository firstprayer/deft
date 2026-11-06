use super::_common::files;
use deft_core::prelude::*;
use serde_json::{Map, Value as Json};

pub static OP: Op = Op {
    id: "file.hash",
    summary: "Checksums: --algorithm md5|sha1|sha256|sha512|blake3|crc32",
    details: &["Directories are walked. --expect fails unless the hash matches."],
    input: Input::any_files(1),
    output: Output::report(),
    params: &[
        param(
            "algorithm",
            Kind::Choice(&["md5", "sha1", "sha256", "sha512", "blake3", "crc32"]),
            "Hash function",
        )
        .default("sha256"),
        param("expect", Kind::Text, "Hash a single file must have"),
    ],
    examples: &[
        "deft file hash report.pdf",
        "deft file hash report.pdf appendix.pdf --algorithm md5",
    ],
    tags: &[
        "checksum",
        "sha256sum",
        "md5sum",
        "verify download",
        "fingerprint",
        "digest",
    ],
    result: &["algorithm", "hash", "files"],
    errors: &[ErrorCode::BadValue],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let name = p.text("algorithm");
    let algorithm = deft_tools_file::Algorithm::parse(&name);
    let all = files(job)?;
    let mut hashes = Map::new();
    let mut last = String::new();
    for f in &all {
        last = deft_tools_file::hash_file(f, algorithm)?;
        hashes.insert(f.display().to_string(), Json::String(last.clone()));
    }
    if let Some(want) = p.opt_text("expect") {
        if all.len() != 1 {
            return Err(bad_value("expect", "needs exactly one file"));
        }
        if !last.eq_ignore_ascii_case(want.trim()) {
            return Err(bad_value("expect", format!("mismatch: the file is {last}")));
        }
    }
    let mut facts = facts![("algorithm", name)];
    if all.len() == 1 {
        facts.insert("hash".into(), Json::String(last));
    } else {
        facts.insert("files".into(), Json::Object(hashes));
    }
    Ok(OpResult::report(facts, ""))
}
