use super::_common::{IN, decode_srgb};
use deft_core::prelude::*;
use deft_tools_image::Fit;

pub static OP: Op = Op {
    id: "image.to-gif",
    summary: "Animate images into a GIF: --fps 2",
    details: &[
        "Each picture is fitted inside the frame, on white. The frame is --size, or the largest picture; one side (200x) sets that side and the other fits the tallest or widest picture.",
    ],
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
    let images = job
        .inputs
        .iter()
        .map(|i| decode_srgb(job, i))
        .collect::<Result<Vec<_>>>()?;
    let (w, h) = match p.opt_size("size") {
        Some(SizeSpec {
            width: Some(w),
            height: Some(h),
            ..
        }) => (w, h),
        // One side or a percentage: each picture at that size, in a frame that holds them all.
        Some(s) => images.iter().fold((1, 1), |(w, h), img| {
            let (iw, ih) = s.target(img.width(), img.height());
            (w.max(iw), h.max(ih))
        }),
        None => (
            images.iter().map(|i| i.width()).max().unwrap_or(1),
            images.iter().map(|i| i.height()).max().unwrap_or(1),
        ),
    };
    let spec = SizeSpec {
        width: Some(w),
        height: Some(h),
        percent: None,
        source: String::new(),
    };
    let mut frames = Vec::new();
    for img in &images {
        let small = deft_tools_image::resize(img, &spec, Fit::Contain, true)?;
        let fitted = deft_tools_image::edit::pad(
            &small,
            w,
            h,
            [255, 255, 255, 255],
            deft_core::value::Position { x: 0.5, y: 0.5 },
        );
        frames.push(fitted.to_rgba8());
    }
    let n = frames.len();
    let delay = (1000.0 / p.float("fps")).round() as u32;
    let bytes = deft_tools_image::edit::encode_gif(frames, delay, p.int("count") as u16)?;
    Ok(OpResult::one(Produced::bytes(bytes, Format::Gif))
        .fact("frames", n)
        .note(format!("{n} frames")))
}

#[cfg(test)]
mod tests {
    use super::super::_common::testing::{picture, png, run};

    #[test]
    fn size_is_the_frame_size() {
        let wide = png(80, 40, [0, 0, 200], None);
        let tall = png(40, 80, [0, 0, 200], None);
        let res = run(
            &super::OP,
            vec![wide.clone(), tall.clone()],
            &[("size", "60x60")],
        );
        assert_eq!(picture(&res).to_rgb8().dimensions(), (60, 60));
        // One side: the other comes from the tallest picture.
        let res = run(&super::OP, vec![wide, tall], &[("size", "30x")]);
        assert_eq!(picture(&res).to_rgb8().dimensions(), (30, 60));
    }
}
