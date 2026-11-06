use super::_common::{
    FFMPEG, VIDEO, duration, ffmpeg, file_result, input_path, probe, stream, strs, temp_out,
};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "video.add-audio",
    summary: "Replace or mix the soundtrack: --audio song.mp3",
    details: &["The picture is copied, not re-encoded. The video's length is kept."],
    input: Input::each(VIDEO, "video"),
    output: Output::one(VIDEO, "video"),
    params: &[
        param("audio", Kind::Path, "Audio file").required(),
        param(
            "mode",
            Kind::Choice(&["replace", "mix"]),
            "Drop the old sound, or keep both",
        )
        .default("replace"),
    ],
    engines: FFMPEG,
    examples: &[
        "deft video add-audio clip.mp4 --audio tone.mp3 -o scored.mp4",
        "deft video add-audio clip.mp4 --audio tone.mp3 --mode mix -o both.mp4",
    ],
    tags: &[
        "soundtrack",
        "background music",
        "dub",
        "voiceover",
        "replace audio",
    ],
    suffix: Some("audio"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let format = job.target(job.input().require_format()?);
    let audio = p.path("audio");
    if !audio.exists() {
        return Err(bad_value("audio", format!("{} not found", audio.display())));
    }
    let info = probe(job, job.input(), OP.id)?;
    let has_sound = stream(&info, "audio").is_some();
    let out = temp_out(job, format);
    let mut args = strs(&["-i"]);
    args.push(input_path(job)?);
    args.extend(strs(&["-i"]));
    args.push(audio.display().to_string());
    if p.text("mode") == "mix" && has_sound {
        args.extend(strs(&[
            "-filter_complex",
            "[0:a][1:a]amix=inputs=2:duration=first:normalize=0[a]",
            "-map",
            "0:v",
            "-map",
            "[a]",
        ]));
    } else {
        args.extend(strs(&["-map", "0:v", "-map", "1:a"]));
    }
    // Longer audio is cut; shorter audio leaves the end silent.
    args.extend(["-t".to_string(), format!("{}", duration(&info))]);
    let (video, sound) = if format == Format::Webm {
        ("copy", "libopus")
    } else {
        ("copy", "aac")
    };
    args.extend(["-c:v".to_string(), video.to_string()]);
    args.extend(["-c:a".to_string(), sound.to_string()]);
    args.push(out.display().to_string());
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, format))
}
