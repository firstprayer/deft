use super::_common::{IN, read, render};
use deft_core::prelude::*;

const OUT: &[Format] = &[Format::Srt, Format::Vtt, Format::Txt, Format::Md];

pub static OP: Op = Op {
    id: "subtitle.convert",
    summary: "Change format: --to srt|vtt|txt|md",
    details: &["txt and md are readable transcripts; auto-caption repeats are removed."],
    input: Input::each(IN, "subtitle"),
    output: Output::one(OUT, "text"),
    params: &[param(
        "to",
        Kind::Choice(&["srt", "vtt", "txt", "md"]),
        "Target format (or give -o name.ext)",
    )],
    converts: &[Edge {
        from: IN,
        to: OUT,
        quality: Quality::Faithful,
    }],
    examples: &[
        "deft subtitle convert talk.srt -o out.vtt",
        "deft subtitle convert talk.vtt --to txt -o transcript.txt",
        "deft subtitle convert talk.ass -o talk-from-ass.srt",
    ],
    tags: &[
        "srt to vtt",
        "vtt to srt",
        "ass to srt",
        "transcript",
        "text",
    ],
    result: &["cues"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let to = job.params.opt_text("to").ok_or_else(|| {
        err(
            ErrorCode::BadOption,
            "--to is required (or give -o name.ext)",
        )
    })?;
    let format = Format::from_name(&to).expect("choice");
    let cues = read(job.input())?;
    Ok(
        OpResult::one(Produced::bytes(render(&cues, format), format))
            .fact("cues", cues.len())
            .note(format!("{} lines", cues.len())),
    )
}
