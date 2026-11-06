use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "html.screenshot",
    summary: "Render a local page to an image with Chrome: --size 1280x800",
    details: &["The page cannot reach the network, so it must be self-contained."],
    input: Input::each(super::_common::IN, "html"),
    output: Output::one(&[Format::Png], "png"),
    params: &[param("size", Kind::Size, "Window size").default("1280x800")],
    engines: &["chrome"],
    examples: &["deft html screenshot page.html --size 800x600 -o shot.png"],
    tags: &["html to image", "html to png", "render page", "capture"],
    suffix: Some("screenshot"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let SizeSpec {
        width: Some(w),
        height: Some(h),
        ..
    } = job.params.size("size")
    else {
        return Err(bad_value("size", "give both sides, e.g. 1280x800"));
    };
    let program = deft_engines::require(job.ctx, "chrome", OP.id)?;
    let out = job.ctx.temp_path("page.png");
    let mut args = deft_engines::browser_args(&job.ctx.temp_path("profile"));
    args.push(format!("--window-size={w},{h}"));
    args.push(format!("--screenshot={}", out.display()));
    args.push(super::_common::file_url(job)?);
    deft_engines::run_browser(job.ctx, &program, &args, &out)?;
    Ok(OpResult::one(Produced::file(out, Some(Format::Png))))
}
