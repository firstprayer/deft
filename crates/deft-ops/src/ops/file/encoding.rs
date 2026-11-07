use super::_common::raw_output;
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "file.encoding",
    summary: "Convert text encoding, normally to UTF-8; fixes line endings",
    details: &[
        "The source encoding is detected unless --from-encoding names it (latin1, gbk, shift_jis...).",
    ],
    input: Input {
        accept: Accept::AnyFile,
        arity: InputArity::Each,
        label: "text",
    },
    output: Output::one(&[], "text"),
    params: &[
        param("to-encoding", Kind::Text, "Target encoding").default("utf-8"),
        param(
            "from-encoding",
            Kind::Text,
            "Source encoding (default: detected)",
        ),
        param("bom", Kind::Flag, "Write a byte order mark"),
        param(
            "line-endings",
            Kind::Choice(&["keep", "lf", "crlf"]),
            "Normalise line endings",
        )
        .default("keep"),
        param("detect", Kind::Flag, "Only report the encoding"),
    ],
    examples: &[
        "deft file encoding latin1.txt -o fixed.txt",
        "deft file encoding notes.md --to-encoding utf-8 --bom --line-endings crlf -o win.md",
    ],
    tags: &[
        "utf-8", "utf8", "iconv", "charset", "gbk", "mojibake", "crlf", "bom",
    ],
    result: &["from", "to"],
    suffix: Some("utf8"),
    run,
    ..Op::DEFAULT
};

fn encoding(option: &str, name: &str) -> Result<&'static deft_tools_file::Encoding> {
    deft_tools_file::find_encoding(name)
        .ok_or_else(|| bad_value(option, format!("unknown encoding {name:?}")))
}

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let input = job.input();
    let bytes = input.bytes()?;
    let from = match p.opt_text("from-encoding") {
        Some(n) => encoding("from-encoding", &n)?,
        None if deft_tools_file::looks_binary(bytes) => {
            return Err(err(
                ErrorCode::InputUnsupported,
                format!("{} is not text", input.display),
            )
            .hint("if it is, name its encoding with --from-encoding"));
        }
        None => deft_tools_file::detect_encoding(bytes),
    };
    if p.flag("detect") {
        return Ok(OpResult::default()
            .fact("from", from.name())
            .note(format!("detected {}", from.name())));
    }
    let to = encoding("to-encoding", &p.text("to-encoding"))?;
    let (mut text, bad) = deft_tools_file::decode_text(bytes, from);
    match p.text("line-endings").as_str() {
        "lf" => text = deft_tools_file::set_line_endings(&text, false),
        "crlf" => text = deft_tools_file::set_line_endings(&text, true),
        _ => {}
    }
    let text = text.strip_prefix('\u{FEFF}').unwrap_or(&text);
    let (out, lost) = deft_tools_file::encode_text(text, to, p.flag("bom"));
    let ext = input
        .path
        .as_ref()
        .and_then(|f| f.extension())
        .map(|e| e.to_string_lossy().to_string())
        .unwrap_or_else(|| "txt".into());
    let name = format!("{}.utf8.{ext}", input.stem());
    let mut res = OpResult::one(raw_output(job, &out, name)?)
        .fact("from", from.name())
        .fact("to", to.name())
        .note(format!("{} -> {}", from.name(), to.name()));
    if bad {
        res = res.warn(format!("some bytes are not valid {}", from.name()));
    }
    if lost {
        res = res.warn(format!("{} cannot hold every character", to.name()));
    }
    Ok(res)
}
