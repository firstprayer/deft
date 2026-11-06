use super::_common::{AUDIO, FFMPEG, ffmpeg, input_path, strs, temp_out};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "audio.compress",
    summary: "Re-encode as MP3 at --bitrate (kbps)",
    input: Input::each(AUDIO, "audio"),
    output: Output::one(&[Format::Mp3], "mp3"),
    params: &[param("bitrate", Kind::Int { min: 8, max: 320 }, "Bitrate in kbps").default("96")],
    engines: FFMPEG,
    examples: &["deft audio compress tone.mp3 --bitrate 64 -o small.mp3"],
    tags: &["shrink", "smaller", "reduce size"],
    result: &["bytes_before", "bytes_after"],
    suffix: Some("compressed"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let out = temp_out(job, Format::Mp3);
    let mut args = strs(&["-i"]);
    args.push(input_path(job)?);
    args.extend([
        "-b:a".to_string(),
        format!("{}k", job.params.int("bitrate")),
        out.display().to_string(),
    ]);
    ffmpeg(job, OP.id, args)?;
    let before = job.input().bytes()?.len() as u64;
    let after = std::fs::metadata(&out)?.len();
    Ok(OpResult::one(Produced::file(out, Some(Format::Mp3)))
        .fact("bytes_before", before)
        .fact("bytes_after", after)
        .note(format!(
            "{} -> {}",
            format_bytes(before),
            format_bytes(after)
        )))
}
