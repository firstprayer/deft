use super::_common::{COOKIES, ENGINES, strs, yt_dlp};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "youtube.info",
    summary: "Title, duration, channel, chapters, qualities",
    input: Input::url(),
    output: Output::report(),
    params: &[COOKIES],
    engines: ENGINES,
    network: true,
    examples: &["deft youtube info https://youtu.be/dQw4w9WgXcQ"],
    tags: &["metadata", "title", "duration", "channel", "details"],
    result: &[
        "title",
        "duration",
        "channel",
        "upload_date",
        "views",
        "chapters",
        "heights",
        "subtitles",
    ],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let out = yt_dlp(
        job,
        OP.id,
        strs(&["--dump-json", "--no-playlist", "--skip-download"]),
    )?;
    let v: serde_json::Value = serde_json::from_slice(&out)
        .map_err(|e| err(ErrorCode::EngineFailed, format!("yt-dlp output: {e}")))?;
    let mut heights: Vec<i64> = v["formats"]
        .as_array()
        .map(|f| f.iter().filter_map(|x| x["height"].as_i64()).collect())
        .unwrap_or_default();
    heights.sort_unstable();
    heights.dedup();
    let subs: Vec<String> = v["subtitles"]
        .as_object()
        .map(|o| o.keys().cloned().collect())
        .unwrap_or_default();
    let title = v["title"].as_str().unwrap_or("unknown").to_string();
    let d = v["duration"].as_f64().unwrap_or(0.0);
    let note = format!("{title}, {}", format_duration(d));
    let facts = facts![
        ("title", title),
        ("duration", d),
        ("channel", v["channel"].as_str().or(v["uploader"].as_str())),
        ("upload_date", v["upload_date"].as_str()),
        ("views", v["view_count"].as_i64()),
        (
            "chapters",
            v["chapters"].as_array().map(Vec::len).unwrap_or(0)
        ),
        ("heights", heights),
        ("subtitles", subs),
    ];
    Ok(OpResult::report(facts, note))
}
