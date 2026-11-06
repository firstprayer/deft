use deft_core::prelude::*;

const FORMATS: &[Format] = &[Format::Jpeg, Format::Png, Format::Webp];

pub static OP: Op = Op {
    id: "image.compress",
    summary: "Smaller file: --quality, --target-size 200KB",
    details: &["JPEG re-encodes. PNG is optimised losslessly (target size does not apply)."],
    input: Input::each(FORMATS, "image"),
    output: Output::one(FORMATS, "image"),
    params: &[
        param("quality", Kind::Int { min: 1, max: 100 }, "JPEG quality").default("75"),
        param(
            "target-size",
            Kind::Bytes,
            "Lower JPEG quality until output <= this size",
        ),
    ],
    examples: &[
        "deft image compress photo-1.jpg -o small.jpg",
        "deft image compress photo-2.jpg --target-size 100KB -o tiny.jpg",
    ],
    tags: &["shrink", "reduce size", "smaller", "optimize", "png", "jpg"],
    result: &["bytes_before", "bytes_after", "ratio", "quality"],
    errors: &[ErrorCode::TargetUnreachable],
    suffix: Some("compressed"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let input = job.input();
    let original = input.bytes()?;
    let format = input.require_format()?;
    let before = original.len() as u64;
    let mut quality = None;
    let data = match format {
        Format::Png => deft_tools_image::optimize_png(original, !job.ctx.keep_metadata)?,
        Format::Jpeg => {
            let img = super::_common::decode(job, job.input())?;
            let encode = |q: u8| {
                deft_tools_image::encode(
                    &img,
                    Format::Jpeg,
                    deft_tools_image::Encode {
                        quality: q,
                        ..Default::default()
                    },
                )
            };
            match p.opt_bytes("target-size") {
                None => {
                    quality = Some(p.int("quality") as u8);
                    encode(p.int("quality") as u8)?
                }
                Some(target) => {
                    // Binary search for the highest quality that fits.
                    let (mut lo, mut hi, mut best) = (5u8, 95u8, None);
                    while lo <= hi {
                        let q = lo + (hi - lo) / 2;
                        let out = encode(q)?;
                        if out.len() as u64 <= target {
                            best = Some((q, out));
                            lo = q + 1;
                        } else {
                            hi = q - 1;
                        }
                    }
                    let (q, out) = best.ok_or_else(|| {
                        err(
                            ErrorCode::TargetUnreachable,
                            format!(
                                "{}: cannot reach {} even at quality 5",
                                input.display,
                                format_bytes(target)
                            ),
                        )
                        .hint("resize it first: deft image resize --size 50%")
                    })?;
                    quality = Some(q);
                    out
                }
            }
        }
        _ => {
            let img = super::_common::decode(job, job.input())?;
            deft_tools_image::encode(&img, format, Default::default())?
        }
    };
    let mut warning = None;
    let data = if data.len() as u64 >= before && p.opt_bytes("target-size").is_none() {
        warning = Some("already compact; output is the original file");
        original.to_vec()
    } else {
        data
    };
    let after = data.len() as u64;
    let ratio = after as f64 / before.max(1) as f64;
    let mut res = OpResult::one(Produced::bytes(data, format))
        .fact("bytes_before", before)
        .fact("bytes_after", after)
        .fact("ratio", (ratio * 1000.0).round() / 1000.0)
        .fact("quality", quality)
        .note(format!(
            "{} -> {} (-{}%)",
            format_bytes(before),
            format_bytes(after),
            ((1.0 - ratio) * 100.0).round() as i64
        ));
    if let Some(w) = warning {
        res = res.warn(w);
    }
    Ok(res)
}
