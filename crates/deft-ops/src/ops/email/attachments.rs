use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "email.attachments",
    summary: "Save the attached files",
    details: &["Attachments that share a name are saved as name-2, name-3."],
    input: Input::each(&[Format::Eml], "eml"),
    output: Output::many(&[], "files"),
    examples: &["deft email attachments message.eml -o files/"],
    tags: &["extract attachment", "save attachment"],
    result: &["files"],
    suffix: Some("attachments"),
    run,
    ..Op::DEFAULT
};

/// A name no earlier attachment has: `image.png`, `image-2.png`, `image-3.png`.
/// Case is ignored, because many disks ignore it too.
fn unique(name: &str, used: &mut std::collections::HashSet<String>) -> String {
    let (stem, extension) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem, format!(".{ext}")),
        _ => (name, String::new()),
    };
    let mut out = name.to_string();
    let mut n = 1;
    while !used.insert(crate::runtime::sanitize(&out).to_lowercase()) {
        n += 1;
        out = format!("{stem}-{n}{extension}");
    }
    out
}

fn run(job: &Job) -> Result<OpResult> {
    let m = deft_tools_doc::email::read(job.input().bytes()?)?;
    let mut outputs = Vec::new();
    let mut used = std::collections::HashSet::new();
    for a in m.attachments {
        let path = job.ctx.temp_path("attachment");
        std::fs::write(&path, &a.bytes)?;
        outputs.push(Produced::file(path, None).named(unique(&a.name, &mut used)));
    }
    let n = outputs.len();
    Ok(OpResult::many(outputs)
        .fact("files", n)
        .note(format!("{n} files")))
}
