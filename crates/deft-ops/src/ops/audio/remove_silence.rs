use super::_common::{AUDIO, FFMPEG, duration, filter_audio, probe};
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
    result: &["removed_seconds"],
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
    let before = duration(&probe(job, job.input(), OP.id)?);
    let mut result = filter_audio(job, OP.id, &filter)?;
    let Some(Data::File(path)) = result.outputs.first().map(|o| &o.data) else {
        return Ok(result);
    };
    let after = duration(&probe(job, &InputFile::file(path.clone(), None), OP.id)?);
    let removed = (before - after).max(0.0);
    result = result.fact("removed_seconds", (removed * 100.0).round() / 100.0);
    Ok(if removed < before * 0.01 {
        result.warn("nothing cut; noise may be above --threshold")
    } else {
        result.note(format!("{removed:.1}s removed"))
    })
}
