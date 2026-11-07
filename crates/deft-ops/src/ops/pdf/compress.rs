use super::_common::{PASSWORD, open};
use deft_core::prelude::*;
use deft_tools_pdf::ImagePolicy;

pub static OP: Op = Op {
    id: "pdf.compress",
    summary: "Make smaller: --level, --target-size 2MB",
    details: &[
        "Levels: lossless rewrites structure only; balanced re-encodes images",
        "at JPEG 75, 150 dpi; strong at JPEG 55, 100 dpi.",
    ],
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::one(&[Format::Pdf], "pdf"),
    params: &[
        param(
            "level",
            Kind::Choice(&["lossless", "balanced", "strong"]),
            "How hard to compress",
        )
        .default("balanced"),
        param(
            "target-size",
            Kind::Bytes,
            "Try stronger settings until output <= this size",
        ),
        param("grayscale", Kind::Flag, "Convert images to grayscale"),
        PASSWORD,
    ],
    examples: &[
        "deft pdf compress photos.pdf --level strong -o small.pdf",
        "deft pdf compress reports/*.pdf -o small/ --target-size 2MB",
    ],
    tags: &["shrink", "reduce size", "smaller", "optimize", "email"],
    result: &[
        "bytes_before",
        "bytes_after",
        "ratio",
        "images_recompressed",
    ],
    errors: &[ErrorCode::InputEncrypted, ErrorCode::TargetUnreachable],
    suffix: Some("compressed"),
    run,
    ..Op::DEFAULT
};

fn attempt(bytes: &[u8], job: &Job, policy: Option<ImagePolicy>) -> Result<(Vec<u8>, usize)> {
    let mut doc = deft_tools_pdf::load(bytes, job.params.opt_text("password").as_deref())?;
    let n = policy
        .map(|p| deft_tools_pdf::recompress_images(&mut doc, p))
        .unwrap_or(0);
    deft_tools_pdf::compress_structure(&mut doc);
    Ok((deft_tools_pdf::meta::save_protected(&mut doc)?, n))
}

/// The error for a target below every attempt. The input itself counts as
/// one: a floor above the file the user has would read as if compressing made
/// it bigger.
fn unreachable(name: &str, best: u64, original: u64, target: u64) -> DeftError {
    let message = if best >= original {
        format!(
            "{name}: the file is already compact ({}); {} is not reachable",
            format_bytes(original),
            format_bytes(target)
        )
    } else {
        format!(
            "{name}: smallest reachable size is {}, above {}",
            format_bytes(best),
            format_bytes(target)
        )
    };
    err(ErrorCode::TargetUnreachable, message)
}

fn run(job: &Job) -> Result<OpResult> {
    let input = job.input();
    open(input, job.params)?; // reports encryption and corruption with the file name
    let original = input.bytes()?;
    let grayscale = job.params.flag("grayscale");
    let policy = |dpi, quality| {
        Some(ImagePolicy {
            dpi,
            quality,
            grayscale,
        })
    };
    let (best, recompressed) = match job.params.opt_bytes("target-size") {
        Some(target) => {
            let mut best: Option<(Vec<u8>, usize)> = None;
            for level in [
                None,
                policy(150.0, 75),
                policy(100.0, 55),
                policy(72.0, 40),
                policy(50.0, 25),
            ] {
                let result = attempt(original, job, level)?;
                let done = result.0.len() as u64 <= target;
                if best.as_ref().is_none_or(|b| result.0.len() < b.0.len()) {
                    best = Some(result);
                }
                if done {
                    break;
                }
            }
            let best = best.expect("at least one attempt");
            if best.0.len() as u64 > target {
                return Err(unreachable(
                    &input.display,
                    best.0.len() as u64,
                    original.len() as u64,
                    target,
                ));
            }
            best
        }
        None => {
            let level = match job.params.text("level").as_str() {
                "lossless" => None,
                "strong" => policy(100.0, 55),
                _ => policy(150.0, 75),
            };
            attempt(original, job, level)?
        }
    };
    let before = original.len() as u64;
    let mut warn = None;
    let data = if best.len() as u64 >= before {
        warn = Some("already compact; output is the original file".to_string());
        original.to_vec()
    } else {
        best
    };
    let after = data.len() as u64;
    let ratio = after as f64 / before.max(1) as f64;
    let change = ((1.0 - ratio) * 100.0).round() as i64;
    let mut res = OpResult::one(Produced::bytes(data, Format::Pdf))
        .fact("bytes_before", before)
        .fact("bytes_after", after)
        .fact("ratio", (ratio * 1000.0).round() / 1000.0)
        .fact("images_recompressed", recompressed)
        .note(format!(
            "{} -> {} (-{change}%)",
            format_bytes(before),
            format_bytes(after)
        ));
    if let Some(w) = warn {
        res = res.warn(w);
    }
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_floor_is_never_above_the_input() {
        let e = unreachable("a.pdf", 3_400_000, 3_088_632, 1_005_000);
        assert!(e.message.contains("already compact"), "{}", e.message);
        assert!(!e.message.contains("3.2MB"), "{}", e.message);
        let e = unreachable("a.pdf", 2_000_000, 3_088_632, 1_005_000);
        assert!(
            e.message.contains("smallest reachable size"),
            "{}",
            e.message
        );
    }
}
