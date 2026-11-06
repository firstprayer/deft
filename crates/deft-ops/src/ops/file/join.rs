use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "file.join",
    summary: "Put parts back together, in the order given",
    input: Input::any_files(2),
    output: Output::one(&[], "file"),
    examples: &["deft file join people.csv people2.csv -o both.csv"],
    tags: &["concatenate", "cat", "reassemble", "parts"],
    result: &["bytes"],
    suffix: Some("joined"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let out = job.ctx.temp_path("joined");
    let mut file = std::fs::File::create(&out)?;
    let mut total = 0;
    for input in &job.inputs {
        let mut source = std::fs::File::open(input.local_path(job.ctx)?)?;
        total += std::io::copy(&mut source, &mut file)?;
    }
    let name = job.input().stem();
    Ok(OpResult::one(Produced::file(out, None).named(name)).fact("bytes", total))
}
