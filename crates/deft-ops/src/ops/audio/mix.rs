use super::_common::{AUDIO, FFMPEG, audio_out, ffmpeg, file_result, strs, temp_out};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "audio.mix",
    summary: "Play files on top of each other, such as voice over music",
    details: &["The result lasts as long as the longest input."],
    input: Input::all(AUDIO, 2, "audio"),
    output: Output::one(AUDIO, "audio"),
    engines: FFMPEG,
    examples: &["deft audio mix tone.mp3 tone2.mp3 -o both.mp3"],
    tags: &["overlay", "layer", "background music", "amix"],
    suffix: Some("mixed"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let format = job.target(job.input().require_format()?);
    let out = temp_out(job, format);
    let mut args = Vec::new();
    for i in &job.inputs {
        args.extend(strs(&["-i"]));
        args.push(i.local_path(job.ctx)?.display().to_string());
    }
    let n = job.inputs.len();
    args.extend([
        "-filter_complex".to_string(),
        format!("amix=inputs={n}:duration=longest:normalize=0"),
    ]);
    args.extend(audio_out(job, OP.id, format, &out)?);
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, format))
}
