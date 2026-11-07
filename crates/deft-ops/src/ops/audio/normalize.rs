use super::_common::{
    AUDIO, FFMPEG, audio_out, ffmpeg, file_result, input_path, probe, stream, strs, temp_out,
};
use deft_core::prelude::*;
use serde_json::Value as Json;

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
    let level = format!("loudnorm=I={}:TP=-1.5:LRA=11", job.params.float("loudness"));
    let mut filter = match measure(job, &level)? {
        Some(measured) => format!("{level}:{measured}:linear=true"),
        None => level,
    };
    // loudnorm works at 192 kHz and leaves the audio there.
    let info = probe(job, job.input(), OP.id)?;
    if let Some(rate) = stream(&info, "audio").and_then(|a| a["sample_rate"].as_str()) {
        filter.push_str(&format!(",aresample={rate}"));
    }
    let mut args = strs(&["-i"]);
    args.push(input_path(job)?);
    args.extend(["-af".to_string(), filter]);
    args.extend(audio_out(job, OP.id, format, &out)?);
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, format))
}

/// First pass: loudnorm's reading of the file, as options for the second pass, which
/// can then hit the target. None for silence, which has no loudness to work from.
fn measure(job: &Job, level: &str) -> Result<Option<String>> {
    let path = deft_engines::require(job.ctx, "ffmpeg", OP.id)?;
    let mut args = strs(&["-hide_banner", "-nostdin", "-i"]);
    args.push(input_path(job)?);
    args.extend(strs(&["-vn", "-af"]));
    args.push(format!("{level}:print_format=json"));
    args.extend(strs(&["-f", "null", "-"]));
    let report = deft_engines::run(job.ctx, &path, &args)?.stderr;
    // The reading is a JSON block among ffmpeg's other output.
    let json = report
        .rfind('{')
        .zip(report.rfind('}'))
        .and_then(|(start, end)| report.get(start..=end))
        .and_then(|block| serde_json::from_str::<Json>(block).ok())
        .ok_or_else(|| err(ErrorCode::EngineFailed, "loudnorm gave no measurement"))?;
    let mut options = Vec::new();
    for (option, key) in [
        ("measured_I", "input_i"),
        ("measured_TP", "input_tp"),
        ("measured_LRA", "input_lra"),
        ("measured_thresh", "input_thresh"),
        ("offset", "target_offset"),
    ] {
        match json[key].as_str().and_then(|v| v.parse::<f64>().ok()) {
            Some(value) if value.is_finite() && value > -99.0 => {
                options.push(format!("{option}={value}"));
            }
            _ => return Ok(None),
        }
    }
    Ok(Some(options.join(":")))
}
