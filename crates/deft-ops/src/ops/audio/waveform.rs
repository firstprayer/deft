use super::_common::{AUDIO, FFMPEG, ffmpeg, ffmpeg_color, input_path, strs};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "audio.waveform",
    summary: "Draw the sound wave as an image: --size 1200x240",
    input: Input::each(AUDIO, "audio"),
    output: Output::one(&[Format::Png], "png"),
    params: &[
        param("size", Kind::Size, "Image size").default("1200x240"),
        param("color", Kind::Color, "Wave colour").default("#336699"),
    ],
    engines: FFMPEG,
    examples: &["deft audio waveform tone.mp3 --size 600x120 --color red -o wave.png"],
    tags: &["visualize", "showwavespic", "audiogram", "picture of sound"],
    suffix: Some("waveform"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let SizeSpec {
        width: Some(w),
        height: Some(h),
        ..
    } = p.size("size")
    else {
        return Err(bad_value("size", "give both sides, e.g. 1200x240"));
    };
    let out = job.ctx.temp_path("wave.png");
    let mut args = strs(&["-i"]);
    args.push(input_path(job)?);
    args.extend([
        "-filter_complex".to_string(),
        format!(
            "showwavespic=s={w}x{h}:colors={}:split_channels=0",
            ffmpeg_color(p.color("color"))
        ),
    ]);
    args.extend(strs(&["-frames:v", "1", "-update", "1"]));
    args.push(out.display().to_string());
    ffmpeg(job, OP.id, args)?;
    Ok(OpResult::one(Produced::file(out, Some(Format::Png))))
}
