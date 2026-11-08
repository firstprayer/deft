//! Helpers shared by the image operations.

use deft_core::prelude::*;
use deft_tools_image::heif;
use deft_tools_image::image::DynamicImage;
use deft_tools_image::image::metadata::Orientation;
use std::path::Path;

/// HEIC and AVIF are read through ffmpeg; everything else is built in.
pub const IN: &[Format] = &[
    Format::Png,
    Format::Jpeg,
    Format::Webp,
    Format::Gif,
    Format::Tiff,
    Format::Bmp,
    Format::Ico,
    Format::Svg,
    Format::Heic,
    Format::Avif,
];
pub const INOUT: &[Format] = &[
    Format::Png,
    Format::Jpeg,
    Format::Webp,
    Format::Gif,
    Format::Tiff,
    Format::Bmp,
    Format::Ico,
    Format::Avif,
];
/// No default: giving it is how a picture asks for lossy WebP.
pub const QUALITY: ParamSpec = param(
    "quality",
    Kind::Int { min: 1, max: 100 },
    "Quality for JPEG, WebP and AVIF (default 85)",
);

pub const FUZZ: ParamSpec = param(
    "fuzz",
    Kind::Float {
        min: 0.0,
        max: 100.0,
    },
    "Colour tolerance in percent",
);

pub fn open(job: &Job) -> Result<(DynamicImage, Format)> {
    let input = job.input();
    Ok((decode(job, input)?, input.require_format()?))
}

/// Decode any input of a job.
pub fn decode(job: &Job, input: &InputFile) -> Result<DynamicImage> {
    let format = input.require_format()?;
    let result = if matches!(format, Format::Heic | Format::Avif) {
        through_ffmpeg(job, input)
    } else {
        deft_tools_image::decode(input.bytes()?, format, job.ctx.limits.max_pixels)
    };
    result.map_err(|mut e| {
        e.message = format!("{}: {}", input.display, e.message);
        e
    })
}

/// How to get the whole picture out of a HEIF file: ffmpeg options that go
/// after `-i`. ffmpeg shows the tiles of a grid and a transparency plane as
/// separate streams, and which stream carries what can differ from the file's
/// own item numbers, so streams are told apart by what they hold.
fn select_picture(job: &Job, src: &Path, alpha: bool) -> Result<Vec<String>> {
    let ffprobe = deft_engines::require(job.ctx, "ffprobe", "image decoding")?;
    let args: Vec<String> = [
        "-hide_banner",
        "-v",
        "error",
        "-show_streams",
        "-show_stream_groups",
        "-of",
        "json",
    ]
    .iter()
    .map(|s| s.to_string())
    .chain([src.display().to_string()])
    .collect();
    let out = deft_engines::run(job.ctx, &ffprobe, &args)?;
    let info: serde_json::Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| err(ErrorCode::EngineFailed, format!("ffprobe: {e}")))?;
    let grid = info["stream_groups"]
        .as_array()
        .map(|g| {
            let grids = || g.iter().filter(|x| x["type"] == "Tile Grid");
            grids()
                .find(|x| x["disposition"]["default"] == 1)
                .or_else(|| grids().next())
        })
        .unwrap_or_default();
    if let Some(grid) = grid {
        return Ok(tile_graph(&grid["components"][0]));
    }
    let streams = info["streams"].as_array().map_or(&[][..], |s| &s[..]);
    let gray =
        |s: &&serde_json::Value| s["pix_fmt"].as_str().is_some_and(|p| p.starts_with("gray"));
    let area = |s: &&serde_json::Value| {
        s["width"].as_u64().unwrap_or(0) * s["height"].as_u64().unwrap_or(0)
    };
    let colour = streams.iter().filter(|s| !gray(s)).max_by_key(area);
    let mask = streams.iter().find(gray);
    let index = |s: &serde_json::Value| s["index"].as_u64().unwrap_or(0);
    Ok(match (colour, mask) {
        (Some(c), Some(m)) if alpha => vec![
            "-filter_complex".into(),
            format!("[0:{}][0:{}]alphamerge[out]", index(c), index(m)),
            "-map".into(),
            "[out]".into(),
        ],
        (Some(c), _) => vec!["-map".into(), format!("0:{}", index(c))],
        _ => Vec::new(),
    })
}

/// Join the tiles of a grid and cut it to the real size.
fn tile_graph(grid: &serde_json::Value) -> Vec<String> {
    let number = |v: &serde_json::Value| v.as_u64().unwrap_or(0);
    let tiles = grid["subcomponents"].as_array().map_or(&[][..], |t| &t[..]);
    let inputs: String = tiles
        .iter()
        .map(|t| format!("[0:{}]", number(&t["stream_index"])))
        .collect();
    let layout: Vec<String> = tiles
        .iter()
        .map(|t| {
            format!(
                "{}_{}",
                number(&t["tile_horizontal_offset"]),
                number(&t["tile_vertical_offset"])
            )
        })
        .collect();
    let crop = format!(
        "crop={}:{}:{}:{}",
        number(&grid["width"]),
        number(&grid["height"]),
        number(&grid["horizontal_offset"]),
        number(&grid["vertical_offset"])
    );
    let graph = if tiles.len() > 1 {
        format!(
            "{inputs}xstack=inputs={}:layout={},{crop}[out]",
            tiles.len(),
            layout.join("|")
        )
    } else {
        format!("{inputs}{crop}[out]")
    };
    vec![
        "-filter_complex".into(),
        graph,
        "-map".into(),
        "[out]".into(),
    ]
}

/// HEIC needs an HEVC decoder, which has no pure-Rust implementation, so
/// both HEIC and AVIF are decoded by ffmpeg. deft reads the container itself
/// and hands ffmpeg the coded tiles as a plain stream: ffmpeg's own HEIF
/// reader cannot open some phones' files and joins tiles wrongly in others.
/// Rotation, mirroring and the clean-aperture crop are applied here.
fn through_ffmpeg(job: &Job, input: &InputFile) -> Result<DynamicImage> {
    let max_pixels = job.ctx.limits.max_pixels;
    let bytes = input.bytes()?;
    let facts = heif::read(bytes).unwrap_or_default();
    if facts.width as u64 * facts.height as u64 > max_pixels {
        return Err(err(
            ErrorCode::LimitExceeded,
            format!(
                "image is {}x{}, over the {max_pixels}-pixel safety limit",
                facts.width, facts.height
            ),
        ));
    }
    let mut img = match heif::picture(bytes) {
        Some(picture) => match from_tiles(job, &picture) {
            Ok(img) => img,
            Err(e) if e.code == ErrorCode::EngineMissing => return Err(e),
            // An unusual stream: ffmpeg's own HEIF reader may still manage.
            Err(_) => through_demuxer(job, input, &facts)?,
        },
        None => through_demuxer(job, input, &facts)?,
    };
    if let Some(o) = Orientation::from_exif(facts.orientation) {
        img.apply_orientation(o);
    }
    Ok(img)
}

/// ffmpeg options that turn decoded pictures into full-range RGB or gray,
/// using the colour coding the file states over the stream's own.
fn to_pixels(coded: &heif::Coded, gray: bool) -> String {
    let mut scale = vec!["out_range=pc".to_string()];
    if let Some(nclx) = coded.nclx {
        scale.push(format!(
            "in_range={}",
            if nclx.full_range { "pc" } else { "tv" }
        ));
        let matrix = match nclx.matrix {
            1 => Some("bt709"),
            4 => Some("fcc"),
            5 => Some("bt470"),
            6 => Some("smpte170m"),
            7 => Some("smpte240m"),
            9 | 10 => Some("bt2020"),
            _ => None,
        };
        if let Some(m) = matrix {
            scale.push(format!("in_color_matrix={m}"));
        }
    }
    format!(
        "scale={},format={}",
        scale.join(":"),
        if gray { "gray" } else { "rgb24" }
    )
}

/// Decode every tile of a coded picture and join them.
fn decode_tiles(job: &Job, coded: &heif::Coded, gray: bool) -> Result<Vec<u8>> {
    let ffmpeg = deft_engines::require(job.ctx, "ffmpeg", "image decoding")?;
    let (name, demuxer) = match coded.codec {
        heif::Codec::Hevc => ("tiles.hevc", "hevc"),
        heif::Codec::Av1 => ("tiles.obu", "obu"),
    };
    let src = job.ctx.temp_path(name);
    let out = job.ctx.temp_path("tiles.raw");
    std::fs::write(&src, &coded.stream)?;
    let args: Vec<String> = [
        "-hide_banner",
        "-nostdin",
        "-loglevel",
        "error",
        "-y",
        "-f",
        demuxer,
        "-i",
    ]
    .iter()
    .map(|s| s.to_string())
    .chain([src.display().to_string()])
    .chain([
        "-fps_mode".into(),
        "passthrough".into(),
        "-vf".into(),
        to_pixels(coded, gray),
        "-f".into(),
        "rawvideo".into(),
        out.display().to_string(),
    ])
    .collect();
    deft_engines::run(job.ctx, &ffmpeg, &args)?;
    let raw = std::fs::read(&out)?;
    heif::join(&raw, coded, if gray { 1 } else { 3 }).ok_or_else(|| {
        err(
            ErrorCode::EngineFailed,
            format!(
                "ffmpeg decoded {} bytes for {} tiles of {}x{}",
                raw.len(),
                coded.tiles,
                coded.tile_width,
                coded.tile_height
            ),
        )
    })
}

fn from_tiles(job: &Job, picture: &heif::Picture) -> Result<DynamicImage> {
    use deft_tools_image::image::{RgbImage, RgbaImage};
    let colour = &picture.colour;
    let rgb = decode_tiles(job, colour, false)?;
    let bad = || err(ErrorCode::Internal, "joined tiles have the wrong size");
    let mut img = match &picture.alpha {
        Some(alpha) => {
            let a = decode_tiles(job, alpha, true)?;
            let rgba: Vec<u8> = rgb
                .chunks_exact(3)
                .zip(a)
                .flat_map(|(p, a)| [p[0], p[1], p[2], a])
                .collect();
            DynamicImage::ImageRgba8(
                RgbaImage::from_raw(colour.width, colour.height, rgba).ok_or_else(bad)?,
            )
        }
        None => DynamicImage::ImageRgb8(
            RgbImage::from_raw(colour.width, colour.height, rgb).ok_or_else(bad)?,
        ),
    };
    if let Some((x, y, w, h)) = picture.crop {
        img = img.crop_imm(x, y, w, h);
    }
    Ok(img)
}

/// The older way: ffmpeg reads the HEIF container itself.
fn through_demuxer(job: &Job, input: &InputFile, facts: &heif::Heif) -> Result<DynamicImage> {
    let ffmpeg = deft_engines::require(job.ctx, "ffmpeg", "image decoding")?;
    let max_pixels = job.ctx.limits.max_pixels;
    let out = job.ctx.temp_path("decoded.png");
    let src = input.local_path(job.ctx)?;
    let select = select_picture(job, &src, facts.alpha)?;
    let mut args: Vec<String> = [
        "-hide_banner",
        "-nostdin",
        "-loglevel",
        "error",
        "-y",
        "-noautorotate",
        // A phone photo has hundreds of tiles, each with its own decoder.
        "-threads",
        "1",
        "-i",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    args.push(src.display().to_string());
    args.extend(select);
    // A temporary file: compressing it takes ten times longer than the decoding.
    args.extend(
        ["-frames:v", "1", "-update", "1", "-compression_level", "0"]
            .iter()
            .map(|s| s.to_string()),
    );
    args.push(out.display().to_string());
    let unreadable = |detail: String| {
        err(
            ErrorCode::InputUnsupported,
            format!(
                "deft cannot read this {} layout: {detail}",
                format_name(input)
            ),
        )
    };
    deft_engines::run(job.ctx, &ffmpeg, &args).map_err(|e| unreadable(e.message))?;
    let img = deft_tools_image::decode(&std::fs::read(&out)?, Format::Png, max_pixels)?;
    if img.width() < facts.width || img.height() < facts.height {
        return Err(unreadable(format!(
            "ffmpeg read {}x{} of a {}x{} picture",
            img.width(),
            img.height(),
            facts.width,
            facts.height
        )));
    }
    Ok(img)
}

/// The colour profile of the inputs, when they all share one.
pub fn profile(job: &Job) -> Option<Vec<u8>> {
    let mut all = job.inputs.iter().map(|i| {
        let bytes = i.bytes().ok()?;
        deft_tools_image::icc_profile(bytes, i.format?)
    });
    let first = all.next()?;
    all.all(|p| p == first).then_some(first).flatten()
}

/// WebP with loss, through ffmpeg: the built-in encoder is lossless only.
pub fn lossy_webp(
    job: &Job,
    img: &DynamicImage,
    quality: u8,
    icc: Option<&[u8]>,
) -> Result<Vec<u8>> {
    let ffmpeg = deft_engines::require(job.ctx, "ffmpeg", "lossy WebP")?;
    let (w, h) = (img.width(), img.height());
    let (pixels, raw) = if img.color().has_alpha() {
        ("rgba", img.to_rgba8().into_raw())
    } else {
        ("rgb24", img.to_rgb8().into_raw())
    };
    let src = job.ctx.temp_path("pixels.raw");
    let out = job.ctx.temp_path("lossy.webp");
    std::fs::write(&src, raw)?;
    let size = format!("{w}x{h}");
    let quality = quality.to_string();
    let args: Vec<String> = [
        "-hide_banner",
        "-nostdin",
        "-loglevel",
        "error",
        "-y",
        "-f",
        "rawvideo",
        "-pixel_format",
        pixels,
        "-video_size",
        &size,
        "-i",
    ]
    .iter()
    .map(|s| s.to_string())
    .chain([src.display().to_string()])
    .chain(
        [
            "-frames:v",
            "1",
            "-c:v",
            "libwebp",
            "-lossless",
            "0",
            "-quality",
            &quality,
        ]
        .iter()
        .map(|s| s.to_string()),
    )
    .chain([out.display().to_string()])
    .collect();
    deft_engines::run(job.ctx, &ffmpeg, &args)?;
    let bytes = std::fs::read(&out)?;
    let icc = icc.filter(|p| deft_tools_image::profile_fits(p, img));
    Ok(icc
        .and_then(|p| deft_tools_image::webp::with_profile(&bytes, p, w, h))
        .unwrap_or(bytes))
}

/// Photos, and anything given a --quality, take lossy WebP. A lossless copy of
/// a photo is twice the size of the JPEG it came from.
fn wants_lossy(job: &Job) -> bool {
    job.params.has("quality")
        || job.inputs.iter().any(|i| match i.format {
            Some(Format::Jpeg | Format::Heic | Format::Avif | Format::Tiff) => true,
            Some(Format::Webp) => i.bytes().is_ok_and(deft_tools_image::webp::is_lossy),
            _ => false,
        })
}

/// With --keep-metadata, put the input's EXIF back into a re-encoded picture.
pub fn keep_metadata(job: &Job, bytes: Vec<u8>, format: Format) -> Vec<u8> {
    let source = job.input().bytes().unwrap_or_default();
    if !job.ctx.keep_metadata || deft_tools_image::exif::read(source).is_empty() {
        return bytes;
    }
    deft_tools_image::exif::copy(source, &bytes, format).unwrap_or(bytes)
}

/// Encode an output file. Returns the bytes and anything the user should know.
pub fn encode(
    job: &Job,
    img: &DynamicImage,
    format: Format,
    opt: deft_tools_image::Encode,
) -> Result<(Vec<u8>, Vec<String>)> {
    let mut warnings = Vec::new();
    let animated = job.inputs.iter().any(|i| {
        let bytes = i.bytes().unwrap_or_default();
        i.format
            .is_some_and(|f| deft_tools_image::is_animated(bytes, f))
    });
    if animated {
        warnings.push("animated: only the first frame was kept".to_string());
    }
    if format == Format::Webp && wants_lossy(job) {
        match lossy_webp(job, img, opt.quality, opt.icc) {
            Ok(bytes) => return Ok((bytes, warnings)),
            Err(e) if matches!(e.code, ErrorCode::EngineMissing | ErrorCode::EngineFailed) => {
                warnings.push(
                    "lossy WebP needs ffmpeg with libwebp; this one is lossless and larger".into(),
                );
            }
            Err(e) => return Err(e),
        }
    }
    let bytes = deft_tools_image::encode(img, format, opt)?;
    Ok((keep_metadata(job, bytes, format), warnings))
}

/// Write in the input's format; formats deft cannot write fall back to PNG.
pub fn save(job: &Job, img: &DynamicImage, format: Format) -> Result<OpResult> {
    let format = job.target(format);
    let format = if deft_tools_image::WRITABLE.contains(&format) {
        format
    } else {
        Format::Png
    };
    let quality = job.params.opt_int("quality").unwrap_or(85) as u8;
    let icc = profile(job);
    let opt = deft_tools_image::Encode {
        quality,
        icc: icc.as_deref(),
        ..Default::default()
    };
    let (bytes, warnings) = encode(job, img, format, opt)?;
    let (w, h) = (img.width(), img.height());
    let mut res = OpResult::one(Produced::bytes(bytes, format))
        .fact("width", w)
        .fact("height", h)
        .note(format!("{w}x{h}"));
    for warning in warnings {
        res = res.warn(warning);
    }
    Ok(res)
}

fn format_name(input: &InputFile) -> String {
    input
        .format
        .map_or_else(|| "image".to_string(), |f| f.to_string().to_uppercase())
}
