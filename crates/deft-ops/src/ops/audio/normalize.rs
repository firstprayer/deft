use super::_common::{AUDIO, FFMPEG, audio_out, ffmpeg, file_result, input_path, strs, temp_out};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "audio.normalize",
    summary: "Even out loudness (EBU R128)",
    input: Input::each(AUDIO, "audio"),
    output: Output::one(AUDIO, "audio"),
    params: &[param(
        "loudness",
        Kind::Float {
            min: -70.0,
            max: -5.0,
        },
        "Target loudness in LUFS",
    )
    .default("-16")],
    engines: FFMPEG,
    examples: &["deft audio normalize tone.mp3 -o level.mp3"],
    tags: &["volume", "loudness", "louder", "podcast"],
    suffix: Some("normalized"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let format = job.target(job.input().require_format()?);
    let out = temp_out(job, format);
    let mut args = strs(&["-i"]);
    args.push(input_path(job)?);
    args.extend([
        "-af".to_string(),
        format!("loudnorm=I={}:TP=-1.5:LRA=11", job.params.float("loudness")),
    ]);
    args.extend(audio_out(job, OP.id, format, &out)?);
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, format))
}
