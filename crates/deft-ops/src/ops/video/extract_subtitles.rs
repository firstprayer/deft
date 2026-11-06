use super::_common::{FFMPEG, VIDEO, ffmpeg, input_path, probe, strs, temp_out};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "video.extract-subtitles",
    summary: "Save an embedded subtitle track: --to srt|vtt",
    input: Input::each(VIDEO, "video"),
    output: Output::one(&[Format::Srt, Format::Vtt], "srt|vtt"),
    params: &[
        param("to", Kind::Choice(&["srt", "vtt"]), "Subtitle format").default("srt"),
        param(
            "track",
            Kind::Int { min: 1, max: 99 },
            "Which subtitle track",
        )
        .default("1"),
    ],
    mounts: &["subtitle from-video"],
    engines: FFMPEG,
    examples: &["deft subtitle from-video clip-with-subs.mp4 -o subs.srt"],
    tags: &["pull captions", "soft subtitles", "cc"],
    errors: &[ErrorCode::InputUnsupported],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let format = Format::from_name(&job.params.text("to")).expect("choice");
    let info = probe(job, job.input(), OP.id)?;
    let tracks = info["streams"]
        .as_array()
        .map(|a| a.iter().filter(|s| s["codec_type"] == "subtitle").count())
        .unwrap_or(0);
    let track = job.params.int("track") as usize;
    if tracks == 0 {
        return Err(err(
            ErrorCode::InputUnsupported,
            format!("{} has no subtitle track", job.input().display),
        ));
    }
    if track > tracks {
        return Err(bad_value(
            "track",
            format!("the file has {tracks} subtitle tracks"),
        ));
    }
    // ffmpeg writes short WebVTT times, so go through SRT and render here.
    let srt = temp_out(job, Format::Srt);
    let mut args = strs(&["-i"]);
    args.push(input_path(job)?);
    args.extend(["-map".to_string(), format!("0:s:{}", track - 1)]);
    args.push(srt.display().to_string());
    ffmpeg(job, OP.id, args)?;
    let text =
        std::fs::read_to_string(&srt).map_err(|e| err(ErrorCode::EngineFailed, e.to_string()))?;
    let cues = deft_tools_text::parse_subtitles(&text);
    let body = if format == Format::Vtt {
        deft_tools_text::cues_to_vtt(&cues)
    } else {
        deft_tools_text::cues_to_srt(&cues)
    };
    Ok(OpResult::one(Produced::bytes(body.into_bytes(), format)).fact("cues", cues.len()))
}
