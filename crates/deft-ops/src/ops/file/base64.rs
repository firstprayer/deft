use super::_common::raw_output;
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "file.base64",
    summary: "Encode to base64, or --decode back to bytes",
    details: &["--data-uri gives data:<type>;base64,... for HTML and CSS."],
    input: Input {
        accept: Accept::AnyFile,
        arity: InputArity::Each,
        label: "file",
    },
    output: Output::one(&[], "file"),
    params: &[
        param("decode", Kind::Flag, "Base64 text in, bytes out"),
        param("data-uri", Kind::Flag, "Write a data URI"),
    ],
    examples: &[
        "deft file base64 logo.png -o logo.b64",
        "deft file base64 logo.png --data-uri -o -",
    ],
    tags: &[
        "encode",
        "decode",
        "data uri",
        "embed image",
        "image to base64",
    ],
    result: &["bytes"],
    suffix: Some("base64"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let input = job.input();
    let bytes = input.bytes()?;
    if p.flag("decode") {
        let text = std::str::from_utf8(bytes).ctx("base64 text")?;
        let data = deft_tools_file::base64_decode(text)
            .map_err(|m| err(ErrorCode::InputCorrupt, format!("not base64: {m}")))?;
        let n = data.len();
        let name = format!("{}.bin", input.stem());
        return Ok(OpResult::one(raw_output(job, &data, name)?).fact("bytes", n));
    }
    let mut text = deft_tools_file::base64_encode(bytes);
    if p.flag("data-uri") {
        let mime = input
            .format
            .map_or("application/octet-stream", Format::mime);
        text = format!("data:{mime};base64,{text}");
    }
    text.push('\n');
    let n = text.len();
    let name = format!("{}.b64", input.stem());
    Ok(OpResult::one(raw_output(job, text.as_bytes(), name)?).fact("bytes", n))
}
