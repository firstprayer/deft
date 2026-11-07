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

/// Pictures fitted by one ffmpeg run; each is an open input with its own threads.
const BATCH: usize = 64;

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
            "Soundtrack; with no timing, the pictures share its length",
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
    let first = upright_size(job.input().bytes()?)
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
    let pattern = fit_pictures(job, w, h)?;
    let out = temp_out(job, format);
    let inputs = job.inputs.len();
    // Read as one sequence at the rate of the pictures, so each lasts exactly its
    // time. Above 30 per second the video follows, or pictures would be dropped.
    let rate = (1.0 / per).ceil().clamp(30.0, 120.0);
    let mut args = vec!["-framerate".to_string(), format!("{}", 1.0 / per)];
    args.extend(strs(&["-start_number", "0", "-i"]));
    args.push(pattern);
    if let Some(sound) = &audio {
        args.extend(strs(&["-i"]));
        args.push(sound.display().to_string());
    }
    // The fitted files are full-range JPEGs; players expect video in the usual range.
    let filter = format!("fps={rate},scale=in_range=full:out_range=limited,format=yuv420p");
    args.extend(["-vf".to_string(), filter]);
    args.extend(strs(&["-map", "0:v"]));
    if audio.is_some() {
        args.extend(strs(&["-map", "1:a"]));
    }
    args.extend(["-t".to_string(), format!("{}", per * inputs as f64)]);
    args.extend(video_codec_args(format, None));
    args.push(out.display().to_string());
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, format).fact("images", inputs))
}

/// Size of a picture as it is shown, after its EXIF orientation.
fn upright_size(bytes: &[u8]) -> Option<(u32, u32)> {
    let (w, h) = deft_tools_image::dimensions(bytes)?;
    Some(if deft_tools_image::exif_orientation(bytes) >= 5 {
        (h, w)
    } else {
        (w, h)
    })
}

/// Fit every picture to one size and pixel format, as numbered files, and return
/// their ffmpeg pattern. ffmpeg loses pictures when either changes within a
/// sequence, and phone photos change both.
fn fit_pictures(job: &Job, w: u32, h: u32) -> Result<String> {
    let dir = job.ctx.temp_path("fitted");
    std::fs::create_dir_all(&dir)?;
    let fit = format!(
        "scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color=black,setsar=1,format=yuvj444p"
    );
    for (batch, inputs) in job.inputs.chunks(BATCH).enumerate() {
        let mut args = Vec::new();
        let mut filter = Vec::new();
        let mut outputs = Vec::new();
        for (n, input) in inputs.iter().enumerate() {
            args.push("-i".to_string());
            args.push(input.local_path(job.ctx)?.display().to_string());
            filter.push(format!("[{n}:v]{fit}[v{n}]"));
            outputs.extend(["-map".to_string(), format!("[v{n}]")]);
            outputs.extend(strs(&["-frames:v", "1", "-update", "1", "-q:v", "2"]));
            let name = format!("{:06}.jpg", batch * BATCH + n);
            outputs.push(dir.join(name).display().to_string());
        }
        args.extend(["-filter_complex".to_string(), filter.join(";")]);
        args.extend(outputs);
        ffmpeg(job, OP.id, args)?;
    }
    Ok(dir.join("%06d.jpg").display().to_string())
}
