use super::_common::{FFMPEG, VIDEO, duration, probe, stream};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "video.info",
    summary: "Duration, size, frame rate, codecs",
    input: Input::each(VIDEO, "video"),
    output: Output::report(),
    engines: FFMPEG,
    examples: &["deft video info clip.mp4"],
    tags: &["duration", "resolution", "codec", "fps", "length"],
    result: &[
        "duration",
        "width",
        "height",
        "fps",
        "video_codec",
        "audio_codec",
        "bitrate",
    ],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let info = probe(job, job.input(), OP.id)?;
    let v = stream(&info, "video");
    let a = stream(&info, "audio");
    let d = duration(&info);
    let (w, h) = (
        v.and_then(|s| s["width"].as_i64()),
        v.and_then(|s| s["height"].as_i64()),
    );
    let fps = v
        .and_then(|s| s["avg_frame_rate"].as_str())
        .and_then(|r| r.split_once('/'))
        .and_then(|(a, b)| {
            Some(a.parse::<f64>().ok()? / b.parse::<f64>().ok().filter(|x| *x > 0.0)?)
        });
    let note = format!(
        "{}, {}x{}",
        format_duration(d),
        w.unwrap_or(0),
        h.unwrap_or(0)
    );
    let bitrate = info["format"]["bit_rate"]
        .as_str()
        .and_then(|s| s.parse::<u64>().ok());
    let facts = facts![
        ("duration", d),
        ("width", w),
        ("height", h),
        ("fps", fps.map(|f| (f * 100.0).round() / 100.0)),
        ("video_codec", v.and_then(|s| s["codec_name"].as_str())),
        ("audio_codec", a.and_then(|s| s["codec_name"].as_str())),
        ("bitrate", bitrate),
    ];
    Ok(OpResult::report(facts, note))
}
