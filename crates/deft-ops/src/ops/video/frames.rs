use super::_common::{FFMPEG, VIDEO, duration, ffmpeg, input_path, probe, strs};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "video.frames",
    summary: "Save frames as images: --fps 1 or --at 0:05,0:10",
    input: Input::each(VIDEO, "video"),
    output: Output::many(&[Format::Png, Format::Jpeg], "png|jpg"),
    params: &[
        param(
            "fps",
            Kind::Float {
                min: 0.001,
                max: 120.0,
            },
            "Frames per second",
        ),
        param("at", Kind::Times, "Exact times"),
        param("to", Kind::Choice(&["png", "jpg"]), "Image format").default("jpg"),
    ],
    engines: FFMPEG,
    examples: &[
        "deft video frames clip.mp4 --at 0:01 -o shots/",
        "deft video frames clip.mp4 --fps 1 --to png -o frames/",
    ],
    tags: &[
        "screenshot",
        "thumbnail",
        "snapshot",
        "still",
        "extract images",
    ],
    result: &["files"],
    suffix: Some("frames"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let format = Format::from_name(&p.text("to")).expect("choice");
    let stem = job.input().stem();
    let src = input_path(job)?;
    let mut outputs = Vec::new();
    match (p.opt_times("at"), p.opt_float("fps")) {
        (Some(_), Some(_)) => return Err(err(ErrorCode::BadOption, "use --at or --fps, not both")),
        (None, None) => return Err(err(ErrorCode::BadOption, "--at or --fps is required")),
        (Some(times), None) => {
            let length = duration(&probe(job, job.input(), OP.id)?);
            for (n, t) in times.iter().enumerate() {
                if *t >= length {
                    return Err(bad_value(
                        "at",
                        format!(
                            "{t}s is past the end of the video ({})",
                            format_duration(length)
                        ),
                    ));
                }
                let out = job.ctx.temp_path(&format!("frame.{}", format.ext()));
                let mut args = vec!["-ss".to_string(), format!("{t}")];
                args.extend(strs(&["-i"]));
                args.push(src.clone());
                args.extend(strs(&["-frames:v", "1", "-update", "1"]));
                args.push(out.display().to_string());
                ffmpeg(job, OP.id, args)?;
                outputs.push(Produced::file(out, Some(format)).named(format!(
                    "{stem}-{:03}.{}",
                    n + 1,
                    format.ext()
                )));
            }
        }
        (None, Some(fps)) => {
            let dir = job.ctx.temp_path("frames");
            std::fs::create_dir_all(&dir)?;
            let pattern = dir.join(format!("{stem}-%04d.{}", format.ext()));
            let mut args = strs(&["-i"]);
            args.push(src);
            args.extend([
                "-vf".to_string(),
                format!("fps={fps}"),
                pattern.display().to_string(),
            ]);
            ffmpeg(job, OP.id, args)?;
            let mut files: Vec<_> = std::fs::read_dir(&dir)?
                .filter_map(|e| e.ok())
                .map(|e| e.path())
                .collect();
            files.sort();
            for f in files {
                let name = f
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                outputs.push(Produced::file(f, Some(format)).named(name));
            }
        }
    }
    let n = outputs.len();
    Ok(OpResult::many(outputs)
        .fact("files", n)
        .note(format!("{n} frame{}", if n == 1 { "" } else { "s" })))
}
