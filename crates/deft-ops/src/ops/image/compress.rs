use deft_core::prelude::*;

const FORMATS: &[Format] = &[Format::Jpeg, Format::Png, Format::Webp];
/// GIF is taken only to be refused with the right advice.
const INPUTS: &[Format] = &[Format::Jpeg, Format::Png, Format::Webp, Format::Gif];
/// Below this JPEG or WebP quality a photo shows blocks and colour shifts.
const LOW_QUALITY: u8 = 30;

pub static OP: Op = Op {
    id: "image.compress",
    summary: "Smaller file: --quality, --target-size 200KB",
    details: &[
        "JPEG and WebP re-encode (WebP needs ffmpeg). PNG is optimised losslessly.",
        "A file is never made larger. Missing --target-size is an error.",
        "A target reached only below quality 30 gives a warning: resizing first keeps it sharper.",
    ],
    input: Input::each(INPUTS, "JPEG, PNG or WebP"),
    output: Output::one(FORMATS, "image"),
    params: &[
        param(
            "quality",
            Kind::Int { min: 1, max: 100 },
            "JPEG and WebP quality",
        )
        .default("75"),
        param(
            "target-size",
            Kind::Bytes,
            "Lower the quality until output <= this size",
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

/// The original bytes, without metadata unless the user wants to keep it.
fn unchanged(job: &Job, original: &[u8], format: Format) -> Vec<u8> {
    let stripped = (!job.ctx.keep_metadata)
        .then(|| deft_tools_image::strip_metadata(original, format, true))
        .flatten();
    stripped.map_or_else(|| original.to_vec(), |s| s.bytes)
}

/// Binary search for the highest quality that fits.
fn fit(target: u64, name: &str, encode: impl Fn(u8) -> Result<Vec<u8>>) -> Result<(u8, Vec<u8>)> {
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
    best.ok_or_else(|| {
        err(
            ErrorCode::TargetUnreachable,
            format!(
                "{name}: cannot reach {} even at quality 5",
                format_bytes(target)
            ),
        )
        .hint("resize it first: deft image resize --size 50%")
    })
}

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let input = job.input();
    let original = input.bytes()?;
    let format = input.require_format()?;
    if format == Format::Gif {
        let animated = deft_tools_image::is_animated(original, format);
        let e = err(
            ErrorCode::InputUnsupported,
            format!(
                "{}: image compress takes JPEG, PNG and WebP; it does not shrink GIF{}",
                input.display,
                if animated {
                    ", and converting an animation keeps only its first frame"
                } else {
                    ""
                }
            ),
        );
        return Err(if animated {
            e
        } else {
            e.hint("a still GIF: deft convert <file> -o <name>.png, then compress that")
        });
    }
    let before = original.len() as u64;
    let target = p.opt_bytes("target-size");
    let mut quality = None;
    let mut warning: Option<String> = None;
    let data = if target.is_some_and(|t| before <= t) {
        warning = Some("already under the target; the picture is unchanged".into());
        unchanged(job, original, format)
    } else {
        let data = if format == Format::Png {
            deft_tools_image::optimize_png(original, !job.ctx.keep_metadata)?
        } else {
            let img = super::_common::decode(job, input)?;
            let icc = super::_common::profile(job);
            let icc = icc.as_deref();
            let encode = |q: u8| {
                if format == Format::Webp {
                    return super::_common::lossy_webp(job, &img, q, icc);
                }
                let opt = deft_tools_image::Encode {
                    quality: q,
                    icc,
                    ..Default::default()
                };
                let bytes = deft_tools_image::encode(&img, Format::Jpeg, opt)?;
                Ok(super::_common::keep_metadata(job, bytes, Format::Jpeg))
            };
            let (q, out) = match target {
                None => (p.int("quality") as u8, encode(p.int("quality") as u8)?),
                Some(t) => fit(t, &input.display, encode)?,
            };
            if target.is_some() && q < LOW_QUALITY {
                warning = Some(format!(
                    "the target needed quality {q}, which shows blocks and shifts colours; \
                     resizing first keeps it sharper: deft image resize --size 50%"
                ));
            }
            quality = Some(q);
            out
        };
        if data.len() as u64 >= before {
            quality = None;
            warning = Some("already compact; the picture is unchanged".into());
            unchanged(job, original, format)
        } else {
            data
        }
    };
    if let Some(t) = target.filter(|t| data.len() as u64 > *t) {
        return Err(err(
            ErrorCode::TargetUnreachable,
            format!(
                "{}: lossless PNG optimisation stops at {}, above {}",
                input.display,
                format_bytes(data.len() as u64),
                format_bytes(t)
            ),
        )
        .hint("convert it to JPEG, or resize it first: deft image resize --size 50%"));
    }
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

#[cfg(test)]
mod tests {
    use super::super::_common::testing::{run, try_run};
    use deft_core::prelude::*;
    use deft_tools_image::image::{DynamicImage, Rgb, RgbImage};

    #[test]
    fn a_target_reached_only_at_low_quality_warns() {
        let noise = DynamicImage::ImageRgb8(RgbImage::from_fn(96, 96, |x, y| {
            let v = (x * 7919 + y * 104_729) ^ (x * y * 31);
            Rgb([v as u8, (v >> 3) as u8, (v >> 6) as u8])
        }));
        let at = |quality| {
            let opt = deft_tools_image::Encode {
                quality,
                ..Default::default()
            };
            deft_tools_image::encode(&noise, Format::Jpeg, opt).unwrap()
        };
        let input = at(95);
        let target = at(12).len().to_string();
        let res = run(
            &super::OP,
            vec![(input.clone(), Format::Jpeg)],
            &[("target-size", &target)],
        );
        assert!(
            res.warnings.iter().any(|w| w.contains("resiz")),
            "{:?}",
            res.warnings
        );
        let target = at(80).len().to_string();
        let res = run(
            &super::OP,
            vec![(input, Format::Jpeg)],
            &[("target-size", &target)],
        );
        assert!(res.warnings.is_empty(), "{:?}", res.warnings);
    }

    #[test]
    fn a_gif_is_refused_without_advice_that_loses_frames() {
        let frame = || deft_tools_image::image::RgbaImage::new(4, 4);
        let gif = deft_tools_image::edit::encode_gif(vec![frame(), frame()], 100, 0).unwrap();
        let e = try_run(&super::OP, vec![(gif, Format::Gif)], &[]).unwrap_err();
        assert_eq!(e.code, ErrorCode::InputUnsupported);
        assert!(e.message.contains("first frame"), "{}", e.message);
        assert!(e.hint.is_none());
    }
}
