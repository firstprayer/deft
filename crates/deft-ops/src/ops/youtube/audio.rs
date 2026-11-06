use super::_common::{COOKIES, file_name, fresh_dir, only_file, strs, template, yt_dlp};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "youtube.audio",
    summary: "Save only the sound: --to mp3|m4a|opus",
    input: Input::url(),
    output: Output::one(&[Format::Mp3, Format::M4a, Format::Opus], "audio"),
    params: &[
        param("to", Kind::Choice(&["mp3", "m4a", "opus"]), "Audio format").default("mp3"),
        COOKIES,
    ],
    engines: &["yt-dlp", "ffmpeg"],
    network: true,
    examples: &["deft youtube audio https://youtu.be/dQw4w9WgXcQ -o song.mp3"],
    tags: &["mp3", "music", "podcast", "sound only"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    deft_engines::require(job.ctx, "ffmpeg", OP.id)?;
    let to = job.params.text("to");
    let format = Format::from_name(&to).expect("choice");
    let dir = fresh_dir(job)?;
    let mut args = strs(&["-x", "--no-playlist", "-f", "ba/b"]);
    args.extend([
        "--audio-format".to_string(),
        to,
        "-o".to_string(),
        template(&dir),
    ]);
    yt_dlp(job, OP.id, args)?;
    let file = only_file(&dir, None)
        .ok_or_else(|| err(ErrorCode::EngineFailed, "yt-dlp finished but wrote no file"))?;
    let name = file_name(&file);
    Ok(OpResult::one(
        Produced::file(file, Some(format)).named(name),
    ))
}
