use super::_common::{COOKIES, ENGINES, fresh_dir, only_file, strs, yt_dlp};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "youtube.subtitles",
    summary: "Transcript: --lang en,zh --to md|txt|srt|vtt",
    details: &["Uses human subtitles when present, else auto-generated ones with repeats removed."],
    input: Input::url(),
    output: Output::one(&[Format::Md, Format::Txt, Format::Srt, Format::Vtt], "text"),
    params: &[
        param("lang", Kind::Text, "Languages in order of preference").default("en"),
        param(
            "to",
            Kind::Choice(&["md", "txt", "srt", "vtt"]),
            "Output format",
        )
        .default("md"),
        COOKIES,
    ],
    engines: ENGINES,
    network: true,
    examples: &[
        "deft youtube subtitles https://youtu.be/dQw4w9WgXcQ -o talk.md",
        "deft youtube subtitles https://youtu.be/dQw4w9WgXcQ --lang zh-Hans,en --to srt -o captions.srt",
    ],
    tags: &[
        "captions",
        "transcript",
        "cc",
        "text of video",
        "summarize video",
    ],
    result: &["language", "cues"],
    errors: &[ErrorCode::InputUnsupported],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let format = Format::from_name(&p.text("to")).expect("choice");
    let langs = p.text("lang");
    let dir = fresh_dir(job)?;
    let mut args = strs(&[
        "--skip-download",
        "--no-playlist",
        "--write-subs",
        "--write-auto-subs",
        "--sub-format",
        "vtt/best",
        "--convert-subs",
        "vtt",
    ]);
    args.extend([
        "--sub-langs".to_string(),
        langs.clone(),
        "-o".to_string(),
        dir.join("subs").display().to_string(),
    ]);
    yt_dlp(job, OP.id, args)?;
    // Pick the first requested language that arrived.
    let mut chosen = None;
    for lang in langs.split(',').map(str::trim) {
        let candidate = dir.join(format!("subs.{lang}.vtt"));
        if candidate.is_file() {
            chosen = Some((candidate, lang.to_string()));
            break;
        }
    }
    let (file, language) =
        match chosen.or_else(|| only_file(&dir, Some("vtt")).map(|f| (f, "unknown".to_string()))) {
            Some(c) => c,
            None => {
                return Err(err(
                    ErrorCode::InputUnsupported,
                    format!("no subtitles found for {langs}"),
                )
                .hint("deft youtube info <url> lists the languages"));
            }
        };
    let raw = std::fs::read_to_string(&file)?;
    let cues = deft_tools_text::parse_subtitles(&raw);
    let text = match format {
        Format::Vtt => raw,
        Format::Srt => deft_tools_text::cues_to_srt(&cues),
        Format::Txt => deft_tools_text::cues_to_text(&cues),
        _ => deft_tools_text::cues_to_markdown(&cues, None),
    };
    let n = cues.len();
    Ok(OpResult::one(
        Produced::bytes(text.into_bytes(), format).named(format!("subtitles.{}", format.ext())),
    )
    .fact("language", language.clone())
    .fact("cues", n)
    .note(format!("{language}, {n} lines")))
}
