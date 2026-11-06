use super::_common::{IN, INOUT, QUALITY, open, save};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "image.pad",
    summary: "Add a border or canvas: --border 20, --size 1080x1080",
    input: Input::each(IN, "image"),
    output: Output::one(INOUT, "image"),
    params: &[
        param(
            "border",
            Kind::Int {
                min: 0,
                max: 100_000,
            },
            "Pixels on each side",
        ),
        param("size", Kind::Size, "Final canvas size"),
        param("background", Kind::Color, "Canvas colour").default("white"),
        param("position", Kind::Position, "Where the picture sits").default("center"),
        QUALITY,
    ],
    examples: &[
        "deft image pad logo.png --border 20 --background red -o framed.png",
        "deft image pad photo-1.jpg --size 1600x1600 --position top -o square.jpg",
    ],
    tags: &["border", "frame", "canvas", "letterbox", "extent", "margin"],
    result: &["width", "height"],
    suffix: Some("padded"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let (img, format) = open(job)?;
    let (w, h) = match (p.opt_int("border"), p.opt_size("size")) {
        (Some(_), Some(_)) => {
            return Err(err(
                ErrorCode::BadOption,
                "use --border or --size, not both",
            ));
        }
        (None, None) => return Err(err(ErrorCode::BadOption, "--border or --size is required")),
        (Some(b), None) => (img.width() + 2 * b as u32, img.height() + 2 * b as u32),
        (None, Some(s)) => {
            let (Some(w), Some(h)) = (s.width, s.height) else {
                return Err(bad_value("size", "give both sides, e.g. 1080x1080"));
            };
            if w < img.width() || h < img.height() {
                return Err(bad_value(
                    "size",
                    format!("smaller than the {}x{} image", img.width(), img.height()),
                ));
            }
            (w, h)
        }
    };
    let out =
        deft_tools_image::edit::pad(&img, w, h, p.color("background"), p.position("position"));
    save(job, &out, format)
}
