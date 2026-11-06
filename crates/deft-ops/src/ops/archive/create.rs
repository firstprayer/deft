use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "archive.create",
    summary: "Pack files and folders: --to zip|tar.gz|tar.xz|7z",
    details: &["--password encrypts a ZIP or 7z with AES-256; tarballs have no passwords."],
    input: Input::any_files(1),
    output: Output::one(
        &[
            Format::Zip,
            Format::Tar,
            Format::TarGz,
            Format::TarBz2,
            Format::TarXz,
            Format::TarZst,
            Format::SevenZ,
        ],
        "archive",
    ),
    params: &[
        param(
            "to",
            Kind::Choice(&["zip", "tar", "tar.gz", "tar.bz2", "tar.xz", "tar.zst", "7z"]),
            "Archive format (or give -o name.ext)",
        )
        .default("zip"),
        param("password", Kind::Text, "Password for a ZIP or 7z"),
    ],
    examples: &[
        "deft archive create report.pdf appendix.pdf -o packed.zip",
        "deft archive create photo-1.jpg --to tar.gz -o packed.tar.gz",
        "deft archive create report.pdf --password s3cret -o locked.zip",
    ],
    tags: &[
        "zip",
        "tar",
        "pack",
        "bundle",
        "compress folder",
        "encrypted zip",
        "xz",
        "zstd",
    ],
    result: &["files", "bytes"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let format = Format::from_name(&job.params.text("to")).expect("choice");
    let paths: Vec<_> = job.inputs.iter().filter_map(|i| i.path.clone()).collect();
    let out = job.ctx.temp_path(&format!("archive.{}", format.ext()));
    let password = job.params.opt_text("password");
    if password.is_some() && !matches!(format, Format::Zip | Format::SevenZ) {
        return Err(bad_value(
            "password",
            "only ZIP and 7z archives can have one",
        ));
    }
    let files = deft_tools_archive::create(&paths, &out, format, password.as_deref())?;
    let name = job.input().stem();
    Ok(
        OpResult::one(Produced::file(out, Some(format)).named(format!("{name}.{}", format.ext())))
            .fact("files", files)
            .note(format!("{files} file{}", if files == 1 { "" } else { "s" })),
    )
}
