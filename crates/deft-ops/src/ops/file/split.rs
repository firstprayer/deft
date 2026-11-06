use deft_core::prelude::*;
use std::io::Read;

pub static OP: Op = Op {
    id: "file.split",
    summary: "Cut a big file into parts: --part-size 100MB",
    details: &["Parts are named <file>.part001 and so on; deft file join restores them."],
    input: Input {
        accept: Accept::AnyFile,
        arity: InputArity::Each,
        label: "file",
    },
    output: Output::many(&[], "parts"),
    params: &[param("part-size", Kind::Bytes, "Largest part").required()],
    examples: &["deft file split photo-1.jpg --part-size 50KB -o parts/"],
    tags: &["chunks", "email limit", "upload limit"],
    result: &["files"],
    suffix: Some("parts"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let size = job.params.bytes("part-size");
    if size == 0 {
        return Err(bad_value("part-size", "must be above zero"));
    }
    let input = job.input();
    let path = input.local_path(job.ctx)?;
    let name = path
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();
    let mut file = std::fs::File::open(&path)?;
    let mut outputs = Vec::new();
    loop {
        let part = job.ctx.temp_path("part");
        let mut out = std::fs::File::create(&part)?;
        let n = std::io::copy(&mut (&mut file).take(size), &mut out)?;
        if n == 0 {
            break;
        }
        let label = format!("{name}.part{:03}", outputs.len() + 1);
        outputs.push(Produced::file(part, None).named(label));
    }
    let n = outputs.len();
    Ok(OpResult::many(outputs)
        .fact("files", n)
        .note(format!("{n} parts")))
}
