use super::_common::{FFMPEG, VIDEO, filter_video, probe, video_size};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "video.crop",
    summary: "Cut a region: --box or --aspect 1:1",
    details: &["Sizes are rounded down to even numbers, as H.264 requires."],
    input: Input::each(VIDEO, "video"),
    output: Output::one(VIDEO, "video"),
    params: &[
        param("box", Kind::Text, "Region in pixels: x,y,width,height"),
        param(
            "aspect",
            Kind::Text,
            "Largest region with this ratio, e.g. 16:9",
        ),
        param("position", Kind::Position, "Anchor for --aspect").default("center"),
    ],
    engines: FFMPEG,
    examples: &[
        "deft video crop clip.mp4 --aspect 1:1 -o square.mp4",
        "deft video crop clip.mp4 --box 0,0,160,120 -o corner.mp4",
    ],
    tags: &["square", "vertical", "portrait", "reframe", "aspect ratio"],
    suffix: Some("cropped"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let info = probe(job, job.input(), OP.id)?;
    let (w, h) =
        video_size(&info).ok_or_else(|| err(ErrorCode::InputUnsupported, "no video stream"))?;
    let (x, y, cw, ch) = match (p.opt_text("box"), p.opt_text("aspect")) {
        (Some(_), Some(_)) => {
            return Err(err(ErrorCode::BadOption, "use --box or --aspect, not both"));
        }
        (None, None) => return Err(err(ErrorCode::BadOption, "--box or --aspect is required")),
        (Some(b), None) => {
            let v: Vec<u32> = b
                .split(',')
                .map(|s| s.trim().parse())
                .collect::<std::result::Result<_, _>>()
                .map_err(|_| bad_value("box", "use x,y,width,height in pixels"))?;
            let [x, y, cw, ch] = v[..] else {
                return Err(bad_value("box", "use x,y,width,height in pixels"));
            };
            if cw < 2 || ch < 2 || x + cw > w || y + ch > h {
                return Err(bad_value(
                    "box",
                    format!("region is outside the {w}x{h} video"),
                ));
            }
            (x, y, cw, ch)
        }
        (None, Some(a)) => {
            let bad = || bad_value("aspect", "use a ratio like 16:9");
            let (aw, ah) = a.split_once([':', '/', 'x']).ok_or_else(bad)?;
            let aw: f64 = aw.trim().parse().map_err(|_| bad())?;
            let ah: f64 = ah.trim().parse().map_err(|_| bad())?;
            if aw <= 0.0 || ah <= 0.0 {
                return Err(bad_value("aspect", "both sides must be positive"));
            }
            deft_tools_image::aspect_box(w, h, aw, ah, p.position("position"))
        }
    };
    let (cw, ch) = (cw & !1, ch & !1);
    filter_video(job, OP.id, &format!("crop={cw}:{ch}:{x}:{y}"))
        .map(|r| r.fact("width", cw).fact("height", ch))
}
