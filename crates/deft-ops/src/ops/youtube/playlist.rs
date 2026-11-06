use super::_common::{COOKIES, ENGINES, strs, yt_dlp};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "youtube.playlist",
    summary: "List the videos of a playlist or channel",
    input: Input::url(),
    output: Output::report(),
    params: &[
        param(
            "limit",
            Kind::Int {
                min: 1,
                max: 100_000,
            },
            "Maximum entries",
        )
        .default("200"),
        COOKIES,
    ],
    engines: ENGINES,
    network: true,
    examples: &["deft youtube playlist https://www.youtube.com/playlist?list=PL123 --limit 20"],
    tags: &["channel", "list videos", "entries"],
    result: &["count", "entries"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let limit = job.params.int("limit");
    let mut args = strs(&["--flat-playlist", "--dump-json", "--skip-download"]);
    args.extend(["--playlist-end".to_string(), limit.to_string()]);
    let out = yt_dlp(job, OP.id, args)?;
    let mut entries = Vec::new();
    for line in String::from_utf8_lossy(&out).lines() {
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
            entries.push(serde_json::json!({"title": v["title"], "url": v["url"], "duration": v["duration"]}));
        }
    }
    let n = entries.len();
    Ok(OpResult::report(
        facts![("count", n), ("entries", entries)],
        format!("{n} videos"),
    ))
}
