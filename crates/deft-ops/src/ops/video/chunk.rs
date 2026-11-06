use super::_common::{FFMPEG, VIDEO, segments, strs, video_codec_args};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "video.chunk",
    summary: "Cut into equal parts: --length 10:00",
    details: &["--fast copies without re-encoding, so cuts land on keyframes."],
    input: Input::each(VIDEO, "video"),
    output: Output::many(VIDEO, "video"),
    params: &[
        param("length", Kind::Duration, "Length of each part").required(),
        param("fast", Kind::Flag, "No re-encoding; cuts land on keyframes"),
    ],
    engines: FFMPEG,
    examples: &["deft video chunk clip.mp4 --length 1 -o parts/"],
    tags: &["split into parts", "segments", "pieces", "every minute"],
    result: &["files"],
    suffix: Some("chunks"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let format = job.input().require_format()?;
    let length = p.seconds("length");
    if length <= 0.0 {
        return Err(bad_value("length", "must be above zero"));
    }
    let codec = if p.flag("fast") {
        strs(&["-c", "copy", "-map", "0"])
    } else {
        let mut c = video_codec_args(format, None);
        c.extend([
            "-force_key_frames".to_string(),
            format!("expr:gte(t,n_forced*{length})"),
            // A forced keyframe can land a hair before the cut after rounding, and
            // the part would then run on to the next one. Half a frame at 10 fps.
            "-segment_time_delta".to_string(),
            "0.05".to_string(),
        ]);
        c
    };
    segments(job, OP.id, length, format, codec)
}
