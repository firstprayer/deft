use super::_common::{
    FFMPEG, VIDEO, duration, ffmpeg, file_result, probe, stream, strs, temp_out, video_codec_args,
    video_size,
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

/// What the filter graph needs to know about one clip.
struct Clip {
    audio: bool,
    length: f64,
}

/// Fits a clip to the frame. ffmpeg has already turned a phone clip upright, so a
/// rotation tag left on the output would turn the whole video again.
fn fit(i: usize, (w, h): (u32, u32), more: &str) -> String {
    format!(
        "[{i}:v]scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2,setsar=1,sidedata=mode=delete:type=DISPLAYMATRIX{more}[v{i}];"
    )
}

/// The label of a clip's sound. A clip with none gets silence of its own length,
/// so that the other clips keep theirs.
fn sound(i: usize, clip: &Clip, filter: &mut String) -> String {
    if clip.audio {
        return format!("{i}:a");
    }
    let length = clip.length;
    filter.push_str(&format!("anullsrc=r=44100:cl=stereo:d={length}[s{i}];"));
    format!("s{i}")
}

fn run(job: &Job) -> Result<OpResult> {
    let out = temp_out(job, Format::Mp4);
    let mut args = Vec::new();
    let mut size = None;
    let mut clips = Vec::new();
    for i in &job.inputs {
        args.extend(strs(&["-i"]));
        args.push(i.local_path(job.ctx)?.display().to_string());
        let info = probe(job, i, OP.id)?;
        size = size.or(video_size(&info));
        clips.push(Clip {
            audio: stream(&info, "audio").is_some(),
            length: duration(&info),
        });
    }
    let size = size.unwrap_or((1280, 720));
    let transition = job.params.text("transition");
    args.extend(if transition == "none" {
        concat_args(&clips, size)
    } else {
        let length = job.params.seconds("transition-time");
        transition_args(job, &clips, size, &transition, length)?
    });
    args.extend(video_codec_args(Format::Mp4, None));
    args.push(out.display().to_string());
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, Format::Mp4))
}

/// Join the clips with the concat filter.
fn concat_args(clips: &[Clip], size: (u32, u32)) -> Vec<String> {
    let audio = clips.iter().any(|c| c.audio);
    let mut filter = String::new();
    let mut joined = String::new();
    for (i, clip) in clips.iter().enumerate() {
        filter.push_str(&fit(i, size, ""));
        joined.push_str(&format!("[v{i}]"));
        if audio {
            let label = sound(i, clip, &mut filter);
            joined.push_str(&format!("[{label}]"));
        }
    }
    let (n, a) = (clips.len(), u8::from(audio));
    let tail = if audio { "[v][a]" } else { "[v]" };
    filter.push_str(&format!("{joined}concat=n={n}:v=1:a={a}{tail}"));
    let mut args = vec!["-filter_complex".to_string(), filter];
    args.extend(strs(&["-map", "[v]"]));
    if audio {
        args.extend(strs(&["-map", "[a]"]));
    }
    args
}

/// Chain xfade filters. Each clip must be longer than the transition.
fn transition_args(
    job: &Job,
    clips: &[Clip],
    size: (u32, u32),
    kind: &str,
    length: f64,
) -> Result<Vec<String>> {
    let effect = match kind {
        "dissolve" => "dissolve",
        "wipe" => "wipeleft",
        "slide" => "slideleft",
        _ => "fade",
    };
    for (input, clip) in job.inputs.iter().zip(clips) {
        if clip.length <= length {
            return Err(bad_value(
                "transition-time",
                format!("{} is only {:.2}s long", input.display, clip.length),
            ));
        }
    }
    let mut filter = String::new();
    for i in 0..clips.len() {
        filter.push_str(&fit(i, size, ",fps=30,format=yuv420p,settb=AVTB"));
    }
    let mut last = "v0".to_string();
    let mut offset = 0.0;
    for i in 1..clips.len() {
        offset += clips[i - 1].length - length;
        let name = format!("x{i}");
        filter.push_str(&format!(
            "[{last}][v{i}]xfade=transition={effect}:duration={length}:offset={offset}[{name}];"
        ));
        last = name;
    }
    let mut maps = vec!["-map".to_string(), format!("[{last}]")];
    if clips.iter().any(|c| c.audio) {
        let mut a = sound(0, &clips[0], &mut filter);
        for (i, clip) in clips.iter().enumerate().skip(1) {
            let next = sound(i, clip, &mut filter);
            let name = format!("a{i}");
            filter.push_str(&format!("[{a}][{next}]acrossfade=d={length}[{name}];"));
            a = name;
        }
        maps.extend(["-map".to_string(), format!("[{a}]")]);
    }
    let mut args = vec![
        "-filter_complex".to_string(),
        filter.trim_end_matches(';').to_string(),
    ];
    args.extend(maps);
    Ok(args)
}
