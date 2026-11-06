use super::_common::{AUDIO, FFMPEG, duration, filter_audio, probe};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "audio.fade",
    summary: "Fade in or out: --fade-in 2 --fade-out 3",
    input: Input::each(AUDIO, "audio"),
    output: Output::one(AUDIO, "audio"),
    params: &[
        param("fade-in", Kind::Duration, "Length of the fade at the start"),
        param("fade-out", Kind::Duration, "Length of the fade at the end"),
    ],
    engines: FFMPEG,
    examples: &["deft audio fade tone.mp3 --fade-in 0.5 --fade-out 1 -o soft.mp3"],
    tags: &["fade out", "fade in", "afade"],
    suffix: Some("faded"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let mut stages = Vec::new();
    if let Some(d) = p.opt_seconds("fade-in") {
        stages.push(format!("afade=t=in:st=0:d={d}"));
    }
    if let Some(d) = p.opt_seconds("fade-out") {
        let length = duration(&probe(job, job.input(), OP.id)?);
        stages.push(format!("afade=t=out:st={}:d={d}", (length - d).max(0.0)));
    }
    if stages.is_empty() {
        return Err(err(ErrorCode::BadOption, "give --fade-in or --fade-out"));
    }
    filter_audio(job, OP.id, &stages.join(","))
}
