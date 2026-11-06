use super::_common::{FFMPEG, VIDEO, ffmpeg_color, filter_video};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "video.pad",
    summary: "Fit into a frame with bars: --size 1080x1920",
    details: &["Keeps the whole picture, unlike crop. The picture is scaled to fit."],
    input: Input::each(VIDEO, "video"),
    output: Output::one(VIDEO, "video"),
    params: &[
        param("size", Kind::Size, "Frame size, e.g. 1080x1920").required(),
        param("background", Kind::Color, "Colour of the bars").default("black"),
    ],
    engines: FFMPEG,
    examples: &["deft video pad clip.mp4 --size 480x640 --background white -o tall.mp4"],
    tags: &[
        "letterbox",
        "pillarbox",
        "vertical",
        "shorts",
        "reels",
        "tiktok",
        "black bars",
        "landscape",
        "portrait",
        "horizontal",
        "vertical",
        "reels",
        "shorts",
        "tiktok",
        "letterbox",
        "pillarbox",
        "padding",
        "bars",
    ],
    suffix: Some("padded"),
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
        return Err(bad_value("size", "give both sides, e.g. 1080x1920"));
    };
    let (w, h) = (w & !1, h & !1);
    let color = ffmpeg_color(p.color("background"));
    let filter = format!(
        "scale={w}:{h}:force_original_aspect_ratio=decrease,pad={w}:{h}:(ow-iw)/2:(oh-ih)/2:color={color},setsar=1"
    );
    filter_video(job, OP.id, &filter)
}
