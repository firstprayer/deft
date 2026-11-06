use super::_common::{IN, INOUT, QUALITY, open, save};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "image.crop",
    summary: "Cut a region: --box x,y,w,h or --aspect 16:9",
    input: Input::each(IN, "image"),
    output: Output::one(INOUT, "image"),
    params: &[
        param("box", Kind::Text, "Region in pixels: x,y,width,height"),
        param(
            "aspect",
            Kind::Text,
            "Largest region with this ratio, e.g. 16:9",
        ),
        param("position", Kind::Position, "Anchor for --aspect").default("center"),
        QUALITY,
    ],
    examples: &[
        "deft image crop photo-1.jpg --aspect 1:1 -o square.jpg",
        "deft image crop photo-2.jpg --box 100,100,800,600 -o part.jpg",
    ],
    tags: &["cut", "trim", "square", "aspect ratio"],
    result: &["width", "height"],
    suffix: Some("cropped"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let (img, format) = open(job)?;
    let (w, h) = (img.width(), img.height());
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
            if cw == 0 || ch == 0 || x + cw > w || y + ch > h {
                return Err(bad_value(
                    "box",
                    format!("region is outside the {w}x{h} image"),
                ));
            }
            (x, y, cw, ch)
        }
        (None, Some(a)) => {
            let (aw, ah) = a
                .split_once([':', '/', 'x'])
                .ok_or_else(|| bad_value("aspect", "use a ratio like 16:9"))?;
            let (aw, ah): (f64, f64) = (
                aw.trim()
                    .parse()
                    .map_err(|_| bad_value("aspect", "use a ratio like 16:9"))?,
                ah.trim()
                    .parse()
                    .map_err(|_| bad_value("aspect", "use a ratio like 16:9"))?,
            );
            if aw <= 0.0 || ah <= 0.0 {
                return Err(bad_value("aspect", "both sides must be positive"));
            }
            deft_tools_image::aspect_box(w, h, aw, ah, p.position("position"))
        }
    };
    save(job, &img.crop_imm(x, y, cw, ch), format)
}
