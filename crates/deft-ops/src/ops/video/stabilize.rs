use super::_common::{FFMPEG, VIDEO, filter_video};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "video.stabilize",
    summary: "Steady shaky footage, or --denoise grain",
    input: Input::each(VIDEO, "video"),
    output: Output::one(VIDEO, "video"),
    params: &[param("denoise", Kind::Flag, "Also reduce grain")],
    engines: FFMPEG,
    examples: &["deft video stabilize clip.mp4 --denoise -o steady.mp4"],
    tags: &["shaky", "deshake", "smooth", "grain", "denoise"],
    suffix: Some("steady"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let filter = if job.params.flag("denoise") {
        "deshake,hqdn3d"
    } else {
        "deshake"
    };
    filter_video(job, OP.id, filter)
}
