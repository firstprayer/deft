use deft_core::prelude::*;
use deft_tools_image::image::DynamicImage;

pub static OP: Op = Op {
    id: "image.frames",
    summary: "Save each frame of an animated GIF as PNG",
    input: Input::each(&[Format::Gif], "gif"),
    output: Output::many(&[Format::Png], "png"),
    examples: &["deft image frames anim.gif -o frames/"],
    tags: &["split gif", "extract frames", "explode gif"],
    result: &["files"],
    suffix: Some("frames"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let frames = deft_tools_image::edit::gif_frames(job.input().bytes()?)?;
    let stem = job.input().stem();
    let n = frames.len();
    let outputs = frames
        .into_iter()
        .enumerate()
        .map(|(i, f)| {
            let img = DynamicImage::ImageRgba8(f);
            let bytes = deft_tools_image::encode(&img, Format::Png, Default::default())?;
            Ok(Produced::bytes(bytes, Format::Png).named(format!("{stem}-{:03}.png", i + 1)))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(OpResult::many(outputs)
        .fact("files", n)
        .note(format!("{n} frames")))
}
