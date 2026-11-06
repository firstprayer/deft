use super::_common::{COOKIES, ENGINES, file_name, fresh_dir, only_file, strs, template, yt_dlp};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "youtube.download",
    summary: "Save a video: --resolution 720p, --to mp4|webm|mkv",
    details: &["Works for any site yt-dlp supports. Respect each site's terms and copyright."],
    input: Input::url(),
    output: Output::one(&[Format::Mp4, Format::Webm, Format::Mkv], "video"),
    params: &[
        param(
            "resolution",
            Kind::Choice(&["best", "2160p", "1440p", "1080p", "720p", "480p", "360p"]),
            "Maximum resolution",
        )
        .default("1080p"),
        param("to", Kind::Choice(&["mp4", "webm", "mkv"]), "Container").default("mp4"),
        COOKIES,
    ],
    mounts: &["video download"],
    engines: ENGINES,
    network: true,
    examples: &[
        "deft youtube download https://youtu.be/dQw4w9WgXcQ --resolution 720p -o talk.mp4",
        "deft video download https://vimeo.com/76979871 -o clips/",
    ],
    tags: &["save video", "yt-dlp", "bilibili", "vimeo", "get"],
    errors: &[ErrorCode::EngineMissing, ErrorCode::EngineFailed],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let to = p.text("to");
    let format = Format::from_name(&to).expect("choice");
    let height: Option<u32> = p
        .text("resolution")
        .strip_suffix('p')
        .and_then(|h| h.parse().ok());
    let limit = height.map(|h| format!("[height<={h}]")).unwrap_or_default();
    let have_ffmpeg = deft_engines::locate(job.ctx, "ffmpeg").is_some();
    let selector = if have_ffmpeg {
        format!("bv*{limit}+ba/b{limit}")
    } else {
        format!("b{limit}")
    };
    let dir = fresh_dir(job)?;
    let mut args = vec!["-f".to_string(), selector, "-o".to_string(), template(&dir)];
    args.extend(strs(&["--no-playlist"]));
    if have_ffmpeg {
        args.extend(["--merge-output-format".to_string(), to.clone()]);
    }
    if format == Format::Mp4 {
        // Prefer codecs that play everywhere over AV1/Opus in MP4.
        args.extend(strs(&["-S", "vcodec:h264,acodec:m4a"]));
    }
    yt_dlp(job, OP.id, args)?;
    let file = only_file(&dir, None)
        .ok_or_else(|| err(ErrorCode::EngineFailed, "yt-dlp finished but wrote no file"))?;
    let actual = Format::from_path(&file).unwrap_or(format);
    let name = file_name(&file);
    let mut res = OpResult::one(Produced::file(file, Some(actual)).named(name));
    if !have_ffmpeg {
        res =
            res.warn("ffmpeg not found: quality is limited to files with audio and video combined");
    }
    Ok(res)
}
