use super::_common::{
    AUDIO, FFMPEG, VIDEO, audio_out, ffmpeg, file_result, input_path, strs, temp_out,
};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "video.extract-audio",
    summary: "Save the soundtrack: --to mp3|m4a|wav|flac|ogg",
    input: Input::each(VIDEO, "video"),
    output: Output::one(AUDIO, "audio"),
    params: &[param(
        "to",
        Kind::Choice(&["mp3", "m4a", "wav", "flac", "ogg", "opus"]),
        "Audio format",
    )
    .default("mp3")],
    mounts: &["audio from-video"],
    converts: &[Edge {
        from: VIDEO,
        to: AUDIO,
        quality: Quality::Faithful,
    }],
    engines: FFMPEG,
    examples: &[
        "deft video extract-audio clip.mp4 -o sound.mp3",
        "deft audio from-video clip.mp4 --to wav -o sound.wav",
    ],
    tags: &["soundtrack", "mp3", "rip audio", "video to audio"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let format = Format::from_name(&job.params.text("to")).expect("choice");
    let out = temp_out(job, format);
    let mut args = strs(&["-i"]);
    args.push(input_path(job)?);
    args.push("-vn".to_string());
    args.extend(audio_out(job, OP.id, format, &out)?);
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, format))
}
