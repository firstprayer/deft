use super::_common::{
    FFMPEG, VIDEO, concat_args, duration, ffmpeg, file_result, probe, stream, strs, temp_out,
    video_codec_args, video_size,
};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "video.merge",
    summary: "Join videos; --transition fades",
    details: &["Sizes are matched to the first video."],
    input: Input::all(VIDEO, 2, "video"),
    output: Output::one(&[Format::Mp4], "mp4"),
    params: &[
        param(
            "transition",
            Kind::Choice(&["none", "fade", "dissolve", "wipe", "slide"]),
            "Effect between clips",
        )
        .default("none"),
        param("transition-time", Kind::Duration, "Length of the effect").default("1"),
    ],
    engines: FFMPEG,
    examples: &[
        "deft video merge clip.mp4 clip2.mp4 -o joined.mp4",
        "deft video merge clip.mp4 clip2.mp4 --transition fade -o faded.mp4",
    ],
    tags: &["concat", "join", "combine", "append", "crossfade", "xfade"],
    suffix: Some("merged"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let out = temp_out(job, Format::Mp4);
    let transition = job.params.text("transition");
    let mut args = if transition == "none" {
        concat_args(job, OP.id, true)?
    } else {
        transition_args(job, &transition, job.params.seconds("transition-time"))?
    };
    args.extend(video_codec_args(Format::Mp4, None));
    args.push(out.display().to_string());
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, Format::Mp4))
}

/// Chain xfade filters. Each clip must be longer than the transition.
fn transition_args(job: &Job, kind: &str, length: f64) -> Result<Vec<String>> {
    let effect = match kind {
        "dissolve" => "dissolve",
        "wipe" => "wipeleft",
        "slide" => "slideleft",
        _ => "fade",
    };
    let mut args = Vec::new();
    let mut size = None;
    let mut lengths = Vec::new();
    let mut audio = true;
    for i in &job.inputs {
        args.extend(strs(&["-i"]));
        args.push(i.local_path(job.ctx)?.display().to_string());
        let info = probe(job, i, OP.id)?;
        audio &= stream(&info, "audio").is_some();
        size = size.or(video_size(&info));
        let d = duration(&info);
        if d <= length {
            return Err(bad_value(
                "transition-time",
                format!("{} is only {d:.2}s long", i.display),
            ));
        }
        lengths.push(d);
    }
    let (w, h) = size.unwrap_or((1280, 720));
    let mut filter = String::new();
    for i in 0..job.inputs.len() {
        filter.push_str(&format!(
            "[{i}:v]scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,setsar=1,fps=30,format=yuv420p,settb=AVTB[v{i}];"
        ));
    }
    let mut last = "v0".to_string();
    let mut offset = 0.0;
    for i in 1..lengths.len() {
        offset += lengths[i - 1] - length;
        let name = format!("x{i}");
        filter.push_str(&format!(
            "[{last}][v{i}]xfade=transition={effect}:duration={length}:offset={offset}[{name}];"
        ));
        last = name;
    }
    let mut maps = vec!["-map".to_string(), format!("[{last}]")];
    if audio {
        let mut a = "0:a".to_string();
        for i in 1..job.inputs.len() {
            let name = format!("a{i}");
            filter.push_str(&format!("[{a}][{i}:a]acrossfade=d={length}[{name}];"));
            a = name;
        }
        maps.extend(["-map".to_string(), format!("[{a}]")]);
    }
    args.extend([
        "-filter_complex".to_string(),
        filter.trim_end_matches(';').to_string(),
    ]);
    args.extend(maps);
    Ok(args)
}
