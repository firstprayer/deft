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
];

pub static OP: Op = Op {
    id: "archive.list",
    summary: "List entries without unpacking",
    input: Input::each(FORMATS, "archive"),
    output: Output::report(),
    examples: &["deft archive list bundle.zip"],
    tags: &["contents", "ls", "show", "what is inside"],
    result: &["files", "bytes", "entries"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let input = job.input();
    let path = input.local_path(job.ctx)?;
    let entries = if input.require_format()? == Format::Rar {
        super::_common::list_rar(job, &path)?
    } else {
        deft_tools_archive::list(&path, input.require_format()?)?
    };
    let total: u64 = entries.iter().map(|e| e.1).sum();
    let names: Vec<&str> = entries.iter().map(|e| e.0.as_str()).collect();
    let note = format!(
        "{} entries, {} unpacked",
        entries.len(),
        format_bytes(total)
    );
    Ok(OpResult::report(
        facts![
            ("files", entries.len()),
            ("bytes", total),
            ("entries", names)
        ],
        note,
    ))
}
