use super::_common::{AUDIO, FFMPEG, audio_out, ffmpeg, file_result, input_path, strs, temp_out};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "audio.convert",
    summary: "Change format: --to mp3|m4a|wav|flac|ogg|opus",
    input: Input::each(AUDIO, "audio"),
    output: Output::one(AUDIO, "audio"),
    params: &[
        param(
            "to",
            Kind::Choice(&["mp3", "m4a", "wav", "flac", "ogg", "opus"]),
            "Target format (or give -o name.ext)",
        ),
        param(
            "channels",
            Kind::Choice(&["mono", "stereo"]),
            "Mix to this layout",
        ),
        param(
            "sample-rate",
            Kind::Int {
                min: 8000,
                max: 192000,
            },
            "Samples per second, e.g. 44100",
        ),
    ],
    converts: &[Edge {
        from: AUDIO,
        to: AUDIO,
        quality: Quality::Faithful,
    }],
    engines: FFMPEG,
    examples: &[
        "deft audio convert tone.mp3 --to wav -o tone.wav",
        "deft audio convert tone.mp3 -o tone.flac",
        "deft audio convert tone.mp3 --channels mono --sample-rate 16000 -o voice.wav",
    ],
    tags: &[
        "format", "wav", "mp3", "flac", "m4a", "resample", "mono", "stereo",
    ],
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
    let out = temp_out(job, format);
    let mut args = strs(&["-i"]);
    args.push(input_path(job)?);
    if let Some(c) = job.params.opt_text("channels") {
        let n = if c == "mono" { "1" } else { "2" };
        args.extend(["-ac".to_string(), n.to_string()]);
    }
    if let Some(r) = job.params.opt_int("sample-rate") {
        args.extend(["-ar".to_string(), r.to_string()]);
    }
    args.extend(audio_out(job, OP.id, format, &out)?);
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, format))
}
