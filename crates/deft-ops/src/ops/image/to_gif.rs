use super::_common::{IN, decode};
use deft_core::prelude::*;
use deft_tools_image::Fit;

pub static OP: Op = Op {
    id: "image.to-gif",
    summary: "Animate images into a GIF: --fps 2",
    details: &["Frames are fitted to the first image (or --size)."],
    input: Input::all(IN, 1, "image"),
    output: Output::one(&[Format::Gif], "gif"),
    params: &[
        param(
            "fps",
            Kind::Float {
                min: 0.01,
                max: 50.0,
            },
            "Frames per second",
        )
        .default("2"),
        param("size", Kind::Size, "Frame size"),
        param(
            "count",
            Kind::Int { min: 0, max: 65535 },
            "Plays; 0 loops forever",
        )
        .default("0"),
    ],
    examples: &["deft image to-gif photo-1.jpg photo-2.jpg --fps 1 --size 200x -o moving.gif"],
    tags: &["animate", "animated gif", "frames to gif", "slideshow"],
    suffix: Some("animated"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let mut frames = Vec::new();
    let mut canvas = None;
    for input in &job.inputs {
        let mut img = decode(job, input)?;
        if canvas.is_none() {
            if let Some(s) = p.opt_size("size") {
                img = deft_tools_image::resize(&img, &s, Fit::Contain, true)?;
            }
            canvas = Some((img.width(), img.height()));
        }
        let (w, h) = canvas.expect("set above");
        let fitted = if (img.width(), img.height()) == (w, h) {
            img
        } else {
            let spec = SizeSpec {
                width: Some(w),
                height: Some(h),
                percent: None,
                source: String::new(),
            };
            let small = deft_tools_image::resize(&img, &spec, Fit::Contain, true)?;
            deft_tools_image::edit::pad(
                &small,
                w,
                h,
                [0, 0, 0, 255],
                deft_core::value::Position { x: 0.5, y: 0.5 },
            )
        };
        frames.push(fitted.to_rgba8());
    }
    let n = frames.len();
    let delay = (1000.0 / p.float("fps")).round() as u32;
    let bytes = deft_tools_image::edit::encode_gif(frames, delay, p.int("count") as u16)?;
    Ok(OpResult::one(Produced::bytes(bytes, Format::Gif))
        .fact("frames", n)
        .note(format!("{n} frames")))
}
