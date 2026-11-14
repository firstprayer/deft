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
        param(
            "size",
            Kind::Size,
            "Final canvas size; a larger picture is scaled down to fit",
        ),
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
            (w, h)
        }
    };
    // A phone photo made into a 1080x1080 post: fit it inside, then pad.
    let fitted = if img.width() > w || img.height() > h {
        let s = (w as f64 / img.width() as f64).min(h as f64 / img.height() as f64);
        let fw = ((img.width() as f64 * s).round() as u32).clamp(1, w);
        let fh = ((img.height() as f64 * s).round() as u32).clamp(1, h);
        deft_tools_image::resize_exact(&img, fw, fh)?
    } else {
        img
    };
    let out =
        deft_tools_image::edit::pad(&fitted, w, h, p.color("background"), p.position("position"));
    save(job, &out, format)
}

#[cfg(test)]
mod tests {
    use super::super::_common::testing::{picture, png, run};

    #[test]
    fn a_larger_picture_is_fitted_then_padded() {
        let res = run(
            &super::OP,
            vec![png(400, 300, [200, 0, 0], None)],
            &[("size", "100x100")],
        );
        let out = picture(&res).to_rgb8();
        assert_eq!(out.dimensions(), (100, 100));
        // 100x75 of red in the middle, white bars above and below.
        assert_eq!(out.get_pixel(50, 50).0, [200, 0, 0]);
        assert_eq!(out.get_pixel(50, 5).0, [255, 255, 255]);
        assert_eq!(out.get_pixel(50, 94).0, [255, 255, 255]);
        assert_eq!(out.get_pixel(0, 50).0, [200, 0, 0]);
    }
}
