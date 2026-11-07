use super::_common::{AUDIO, FFMPEG, duration, probe, stream};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "audio.info",
    summary: "Duration, codec, sample rate, channels",
    input: Input::each(AUDIO, "audio"),
    output: Output::report(),
    engines: FFMPEG,
    examples: &["deft audio info tone.mp3"],
    tags: &["duration", "bitrate", "length", "codec"],
    result: &[
        "duration",
        "codec",
        "sample_rate",
        "channels",
        "bitrate",
        "tags",
    ],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let info = probe(job, job.input(), OP.id)?;
    let a = stream(&info, "audio");
    let d = duration(&info);
    let codec = a
        .and_then(|s| s["codec_name"].as_str())
        .unwrap_or("unknown");
    let bitrate = info["format"]["bit_rate"]
        .as_str()
        .and_then(|s| s.parse::<u64>().ok());
    // Ogg keeps its tags on the stream, in capitals.
    let tags: serde_json::Map<String, serde_json::Value> = info["format"]["tags"]
        .as_object()
        .filter(|t| !t.is_empty())
        .or(a.and_then(|s| s["tags"].as_object()))
        .into_iter()
        .flatten()
        .map(|(k, v)| (k.to_lowercase(), v.clone()))
        .collect();
    let facts = facts![
        ("duration", d),
        ("codec", codec),
        (
            "sample_rate",
            a.and_then(|s| s["sample_rate"].as_str())
                .and_then(|s| s.parse::<u64>().ok())
        ),
        ("channels", a.and_then(|s| s["channels"].as_i64())),
        ("bitrate", bitrate),
        ("tags", (!tags.is_empty()).then_some(tags)),
    ];
    Ok(OpResult::report(
        facts,
        format!("{}, {codec}", format_duration(d)),
    ))
}
