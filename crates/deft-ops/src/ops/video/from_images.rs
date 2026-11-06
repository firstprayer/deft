use super::_common::{
    FFMPEG, duration, ffmpeg, file_result, probe, strs, temp_out, video_codec_args,
};
use deft_core::prelude::*;

const IN: &[Format] = &[
    Format::Png,
    Format::Jpeg,
    Format::Webp,
    Format::Bmp,
    Format::Tiff,
];

pub static OP: Op = Op {
    id: "video.from-images",
    summary: "Slideshow: --duration, --fps, --audio",
    details: &["Images are fitted to the size of the first; order is as given."],
    input: Input::all(IN, 1, "image"),
    output: Output::one(
        &[Format::Mp4, Format::Webm, Format::Mov, Format::Mkv],
        "video",
    ),
    params: &[
        param("duration", Kind::Duration, "Seconds per image"),
        param(
            "fps",
            Kind::Float {
                min: 0.001,
                max: 120.0,
            },
            "Images per second",
        ),
        param(
            "audio",
            Kind::Path,
            "Soundtrack; the video stops at its end",
        ),
        param("size", Kind::Size, "Frame size, e.g. 1280x720"),
    ],
    mounts: &["image to-video"],
    engines: FFMPEG,
    examples: &[
        "deft video from-images photo-1.jpg photo-2.jpg --duration 2 -o show.mp4",
        "deft video from-images photo-1.jpg --duration 3 --audio tone.mp3 -o still.mp4",
    ],
    tags: &[
        "images to video",
        "slideshow",
        "timelapse",
        "picture to video",
        "image sequence",
    ],
    suffix: Some("video"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let format = job.target(Format::Mp4);
    let audio = p.opt_path("audio");
    if let Some(a) = &audio
        && !a.exists()
    {
        return Err(bad_value("audio", format!("{} not found", a.display())));
    }
    let per = match (p.opt_seconds("duration"), p.opt_float("fps")) {
        (Some(_), Some(_)) => {
            return Err(err(
                ErrorCode::BadOption,
                "use --duration or --fps, not both",
            ));
        }
        (Some(d), None) if d > 0.0 => d,
        (Some(_), None) => return Err(bad_value("duration", "must be above zero")),
        (None, Some(f)) => 1.0 / f,
        (None, None) => match &audio {
            // No timing given: the pictures share the length of the soundtrack.
            Some(a) => {
                let sound = InputFile::file(a.clone(), None);
                let length = duration(&probe(job, &sound, OP.id)?);
                (length / job.inputs.len() as f64).max(0.1)
            }
            None => 2.0,
        },
    };
    let first = deft_tools_image::dimensions(job.input().bytes()?)
        .ok_or_else(|| corrupt(&job.input().display, "unreadable image"))?;
    let (w, h) = match p.opt_size("size") {
        Some(SizeSpec {
            width: Some(w),
            height: Some(h),
            ..
        }) => (w, h),
        Some(_) => return Err(bad_value("size", "give both sides, e.g. 1280x720")),
        None => first,
    };
    let (w, h) = (w & !1, h & !1);
    let fit = format!(
        "scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color=black,setsar=1,fps=30,format=yuv420p"
    );
    let same_format = job.inputs.iter().all(|i| i.format == job.input().format);
    let out = temp_out(job, format);
    let mut args;
    let mut picture = "0:v".to_string();
    let mut filter = fit.clone();
    if same_format && job.inputs.len() > 1 {
        // The concat demuxer copes with thousands of frames, but one format only.
        let mut list = String::new();
        for input in &job.inputs {
            let path = std::path::absolute(input.local_path(job.ctx)?)?;
            let quoted = path.display().to_string().replace('\'', "'\\''");
            list.push_str(&format!("file '{quoted}'\nduration {per}\n"));
        }
        let list_path = job.ctx.temp_path("images.txt");
        std::fs::write(&list_path, list)?;
        args = strs(&["-f", "concat", "-safe", "0", "-i"]);
        args.push(list_path.display().to_string());
    } else {
        args = Vec::new();
        filter.clear();
        let mut labels = String::new();
        for (n, input) in job.inputs.iter().enumerate() {
            args.extend(strs(&["-loop", "1", "-t"]));
            args.push(format!("{per}"));
            args.push("-i".to_string());
            args.push(input.local_path(job.ctx)?.display().to_string());
            filter.push_str(&format!("[{n}:v]{fit}[v{n}];"));
            labels.push_str(&format!("[v{n}]"));
        }
        let n = job.inputs.len();
        filter.push_str(&format!("{labels}concat=n={n}:v=1:a=0[out]"));
        picture = "[out]".to_string();
    }
    let inputs = job.inputs.len();
    let audio_index = if picture == "[out]" { inputs } else { 1 };
    if let Some(sound) = audio {
        args.extend(strs(&["-i"]));
        args.push(sound.display().to_string());
    }
    let filter_flag = if picture == "[out]" {
        "-filter_complex"
    } else {
        "-vf"
    };
    args.extend([filter_flag.to_string(), filter]);
    args.extend(["-map".to_string(), picture]);
    if p.opt_path("audio").is_some() {
        args.extend(["-map".to_string(), format!("{audio_index}:a")]);
    }
    args.extend(["-t".to_string(), format!("{}", per * inputs as f64)]);
    args.extend(video_codec_args(format, None));
    args.push(out.display().to_string());
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, format).fact("images", job.inputs.len()))
}
