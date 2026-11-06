use super::_common::{IN, INOUT, QUALITY, decode, save};
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
        .map(|i| decode(job, i))
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
    save(job, &out, format)
}
