use super::_common::{
    FFMPEG, VIDEO, duration, ffmpeg, file_result, input_path, probe, stream, strs, temp_out,
    video_codec_args,
};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "video.fade",
    summary: "Fade: --fade-in 1 --fade-out 2",
    input: Input::each(VIDEO, "video"),
    output: Output::one(VIDEO, "video"),
    params: &[
        param("fade-in", Kind::Duration, "Length of the fade at the start"),
        param("fade-out", Kind::Duration, "Length of the fade at the end"),
    ],
    engines: FFMPEG,
    examples: &["deft video fade clip.mp4 --fade-in 0.5 --fade-out 1 -o soft.mp4"],
    tags: &["fade to black", "fade out", "intro", "outro"],
    suffix: Some("faded"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let (fin, fout) = (p.opt_seconds("fade-in"), p.opt_seconds("fade-out"));
    if fin.is_none() && fout.is_none() {
        return Err(err(ErrorCode::BadOption, "give --fade-in or --fade-out"));
    }
    let info = probe(job, job.input(), OP.id)?;
    let length = duration(&info);
    let (mut vf, mut af) = (Vec::new(), Vec::new());
    if let Some(d) = fin {
        vf.push(format!("fade=t=in:st=0:d={d}"));
        af.push(format!("afade=t=in:st=0:d={d}"));
    }
    if let Some(d) = fout {
        let start = (length - d).max(0.0);
        vf.push(format!("fade=t=out:st={start}:d={d}"));
        af.push(format!("afade=t=out:st={start}:d={d}"));
    }
    let format = job.target(job.input().require_format()?);
    let out = temp_out(job, format);
    let mut args = strs(&["-i"]);
    args.push(input_path(job)?);
    args.extend(["-vf".to_string(), vf.join(",")]);
    if stream(&info, "audio").is_some() {
        args.extend(["-af".to_string(), af.join(",")]);
    }
    args.extend(video_codec_args(format, None));
    args.push(out.display().to_string());
    ffmpeg(job, OP.id, args)?;
    Ok(file_result(out, format))
}
