use super::_common::{COOKIES, ENGINES, file_name, fresh_dir, only_file, strs, template, yt_dlp};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "youtube.thumbnail",
    summary: "Save the cover image",
    input: Input::url(),
    output: Output::one(&[Format::Jpeg, Format::Webp, Format::Png], "image"),
    params: &[COOKIES],
    engines: ENGINES,
    network: true,
    examples: &["deft youtube thumbnail https://youtu.be/dQw4w9WgXcQ -o cover.jpg"],
    tags: &["cover", "poster", "image"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let dir = fresh_dir(job)?;
    let mut args = strs(&[
        "--skip-download",
        "--no-playlist",
        "--write-thumbnail",
        "--convert-thumbnails",
        "jpg",
    ]);
    args.extend(["-o".to_string(), template(&dir)]);
    yt_dlp(job, OP.id, args)?;
    let file = only_file(&dir, None)
        .ok_or_else(|| err(ErrorCode::EngineFailed, "no thumbnail was written"))?;
    let format = Format::from_path(&file);
    let name = file_name(&file);
    Ok(OpResult::one(Produced::file(file, format).named(name)))
}
