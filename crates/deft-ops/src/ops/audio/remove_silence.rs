use super::_common::{AUDIO, FFMPEG, filter_audio};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "audio.remove-silence",
    summary: "Cut silent parts: --threshold -40 --min-silence 0.5",
    details: &["--edges-only trims the start and end and leaves pauses alone."],
    input: Input::each(AUDIO, "audio"),
    output: Output::one(AUDIO, "audio"),
    params: &[
        param(
            "threshold",
            Kind::Float {
                min: -120.0,
                max: 0.0,
            },
            "Quieter than this many dB counts as silence",
        )
        .default("-40"),
        param("min-silence", Kind::Duration, "Shortest silence to cut").default("0.5"),
        param("edges-only", Kind::Flag, "Only trim the start and end"),
    ],
    engines: FFMPEG,
    examples: &["deft audio remove-silence gaps.wav -o tight.wav"],
    tags: &[
        "trim silence",
        "pauses",
        "dead air",
        "silenceremove",
        "podcast",
    ],
    suffix: Some("nosilence"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let db = p.float("threshold");
    let d = p.seconds("min-silence");
    let filter = if p.flag("edges-only") {
        let head = format!("silenceremove=start_periods=1:start_threshold={db}dB");
        format!("{head},areverse,{head},areverse")
    } else {
        format!(
            "silenceremove=start_periods=1:start_threshold={db}dB:stop_periods=-1:stop_duration={d}:stop_threshold={db}dB"
        )
    };
    filter_audio(job, OP.id, &filter)
}
