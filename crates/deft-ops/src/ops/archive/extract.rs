use deft_core::prelude::*;

const FORMATS: &[Format] = &[
    Format::Zip,
    Format::Tar,
    Format::TarGz,
    Format::TarBz2,
    Format::TarXz,
    Format::TarZst,
    Format::SevenZ,
    Format::Rar,
    Format::Gz,
];

pub static OP: Op = Op {
    id: "archive.extract",
    summary: "Unpack into a folder",
    details: &[
        "Entries that would escape the target folder are rejected.",
        "RAR goes through bsdtar, because no open library reads every RAR version.",
    ],
    input: Input::each(FORMATS, "archive"),
    output: Output::dir(),
    params: &[param("password", Kind::Text, "Password of a ZIP or 7z")],
    examples: &["deft archive extract bundle.zip -o unpacked/"],
    tags: &[
        "unzip",
        "untar",
        "unpack",
        "decompress",
        "open zip",
        "7z",
        "xz",
        "bz2",
        "zstd",
    ],
    result: &["files"],
    errors: &[ErrorCode::LimitExceeded, ErrorCode::InputEncrypted],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let input = job.input();
    let dir = job.ctx.temp_path("extracted");
    std::fs::create_dir_all(&dir)?;
    let path = input.local_path(job.ctx)?;
    if input.require_format()? == Format::Rar {
        return super::_common::extract_rar(job, &path, dir);
    }
    let files = deft_tools_archive::extract(
        &path,
        input.require_format()?,
        &dir,
        job.ctx.limits.max_unpacked_bytes,
        job.params.opt_text("password").as_deref(),
    )?;
    Ok(OpResult::one(Produced::dir(dir))
        .fact("files", files)
        .note(format!("{files} file{}", if files == 1 { "" } else { "s" })))
}
