use super::_common::files;
use deft_core::prelude::*;
use std::path::Path;

pub static OP: Op = Op {
    id: "file.organize",
    summary: "Copy files into folders: --group type|ext|month",
    details: &["Originals stay where they are. Name clashes get a number."],
    input: Input::any_files(1),
    output: Output::dir(),
    params: &[param(
        "group",
        Kind::Choice(&["type", "ext", "month"]),
        "What each folder collects",
    )
    .default("type")],
    examples: &["deft file organize photo-1.jpg report.pdf notes.md -o sorted"],
    tags: &[
        "sort downloads",
        "tidy folder",
        "by date",
        "by extension",
        "folders",
    ],
    result: &["files"],
    run,
    ..Op::DEFAULT
};

fn kind(path: &Path) -> &'static str {
    let format = deft_core::format::sniff_file(path).ok().flatten();
    match format {
        Some(f) if f.is_image() => "images",
        Some(f) if f.is_video() => "videos",
        Some(f) if f.is_audio() => "audio",
        Some(Format::Pdf | Format::Docx | Format::Odt | Format::Rtf | Format::Txt | Format::Md) => {
            "documents"
        }
        Some(Format::Pptx | Format::Odp) => "documents",
        Some(Format::Xlsx | Format::Xls | Format::Ods | Format::Csv) => "spreadsheets",
        Some(Format::Zip | Format::Tar | Format::TarGz | Format::Gz | Format::SevenZ) => "archives",
        _ => "other",
    }
}

/// `YYYY-MM` of a modification time.
fn month(path: &Path) -> Result<String> {
    let modified = std::fs::metadata(path)?.modified()?;
    let days = modified
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / 86400) as i64;
    // Civil-from-days (Howard Hinnant).
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    Ok(format!("{y:04}-{m:02}"))
}

fn run(job: &Job) -> Result<OpResult> {
    let group = job.params.text("group");
    let dest = job.ctx.temp_path("organized");
    let all = files(job)?;
    for f in &all {
        let folder = match group.as_str() {
            "ext" => f
                .extension()
                .map_or("none".to_string(), |e| e.to_string_lossy().to_lowercase()),
            "month" => month(f)?,
            _ => kind(f).to_string(),
        };
        let dir = dest.join(folder);
        std::fs::create_dir_all(&dir)?;
        let name = f
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        let mut target = dir.join(&name);
        let mut n = 1;
        while target.exists() {
            n += 1;
            let stem = f.file_stem().unwrap_or_default().to_string_lossy();
            target = match f.extension() {
                Some(e) => dir.join(format!("{stem}-{n}.{}", e.to_string_lossy())),
                None => dir.join(format!("{stem}-{n}")),
            };
        }
        std::fs::copy(f, target)?;
    }
    Ok(OpResult::one(Produced::dir(dest))
        .fact("files", all.len())
        .note(format!("{} files", all.len())))
}
