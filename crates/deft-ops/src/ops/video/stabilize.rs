use super::_common::{FFMPEG, VIDEO, ffmpeg, filter_video, input_path, strs};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "video.stabilize",
    summary: "Steady shaky footage, or --denoise grain",
    input: Input::each(VIDEO, "video"),
    output: Output::one(VIDEO, "video"),
    params: &[param("denoise", Kind::Flag, "Also reduce grain")],
    engines: FFMPEG,
    examples: &["deft video stabilize clip.mp4 --denoise -o steady.mp4"],
    tags: &["shaky", "deshake", "smooth", "grain", "denoise"],
    suffix: Some("steady"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let steady = two_pass(job).unwrap_or_else(|_| "deshake".to_string());
    let filter = if job.params.flag("denoise") {
        format!("{steady},hqdn3d")
    } else {
        steady
    };
    filter_video(job, OP.id, &filter)
}

/// Measures the shake first, which removes far more of it than ffmpeg's one-pass
/// `deshake`. Some ffmpeg builds lack this filter, and they fall back to that.
fn two_pass(job: &Job) -> Result<String> {
    let motions = job.ctx.temp_path("motions.trf");
    // In a filter, `:` separates options and `\\` starts an escape.
    let motions = motions
        .display()
        .to_string()
        .replace('\\', "/")
        .replace(':', "\\:");
    let mut args = strs(&["-i"]);
    args.push(input_path(job)?);
    args.extend([
        "-vf".to_string(),
        format!("vidstabdetect=shakiness=8:accuracy=15:result={motions}"),
    ]);
    args.extend(strs(&["-f", "null", "-"]));
    ffmpeg(job, OP.id, args)?;
    Ok(format!(
        "vidstabtransform=input={motions}:smoothing=30:optzoom=1"
    ))
}
