use super::_common::{IN, INOUT, QUALITY, decode_srgb, save_srgb};
use deft_core::prelude::*;
use deft_tools_image::edit::Grid;

pub static OP: Op = Op {
    id: "image.montage",
    summary: "Tile images into a grid: --cols 3 --size 300x300",
    details: &["--cols 1 stacks vertically; --cols equal to the count puts them in one row."],
    input: Input::all(IN, 1, "image"),
    output: Output::one(INOUT, "image"),
    params: &[
        param(
            "cols",
            Kind::Int { min: 1, max: 1000 },
            "Columns (default: a square grid)",
        ),
        param("size", Kind::Size, "Size of each tile"),
        param(
            "gap",
            Kind::Int {
                min: 0,
                max: 10_000,
            },
            "Pixels between tiles",
        )
        .default("0"),
        param("background", Kind::Color, "Colour behind the tiles").default("white"),
        QUALITY,
    ],
    examples: &[
        "deft image montage photo-1.jpg photo-2.jpg photo-3.jpg --cols 2 -o grid.jpg",
        "deft image montage photo-1.jpg photo-2.jpg --size 200x200 --gap 10 -o pair.png",
    ],
    tags: &[
        "collage",
        "contact sheet",
        "side by side",
        "combine images",
        "stack",
        "grid",
    ],
    result: &["width", "height"],
    suffix: Some("montage"),
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
    let n = images.len() as u32;
    let cols = p
        .opt_int("cols")
        .map_or((n as f64).sqrt().ceil() as u32, |c| c as u32);
    let tile = match p.opt_size("size") {
        Some(SizeSpec {
            width: Some(w),
            height: Some(h),
            ..
        }) => Some((w, h)),
        Some(_) => return Err(bad_value("size", "give both sides, e.g. 300x300")),
        None => None,
    };
    let grid = Grid {
        cols,
        tile,
        gap: p.int("gap") as u32,
        background: p.color("background"),
    };
    let out = deft_tools_image::edit::montage(&images, grid)?;
    let format = job.input().require_format()?;
    save_srgb(job, &out, format)
}

#[cfg(test)]
mod tests {
    use super::super::_common::testing::{picture, png, run};

    #[test]
    fn tiles_in_other_colour_spaces_are_converted() {
        let p3 = moxcms::ColorProfile::new_display_p3().encode().unwrap();
        let colour = [200, 100, 50];
        let res = run(
            &super::OP,
            vec![png(10, 10, colour, Some(&p3)), png(10, 10, colour, None)],
            &[("cols", "2")],
        );
        let out = picture(&res).to_rgb8();
        // The Display P3 tile shows a redder orange than the same numbers in sRGB.
        let (p3_tile, srgb_tile) = (out.get_pixel(5, 5).0, out.get_pixel(15, 5).0);
        assert_eq!(srgb_tile, colour);
        assert!(p3_tile[0] > 210 && p3_tile[1] < 95, "{p3_tile:?}");
    }
}
