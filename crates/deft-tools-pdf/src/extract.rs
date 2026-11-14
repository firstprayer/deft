//! Embedded images, taken out without re-rendering the page.

use crate::{deref, num};
use image::{DynamicImage, GrayImage, RgbImage, RgbaImage};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use std::collections::HashSet;

/// One embedded image.
pub enum Extracted {
    /// The original JPEG, byte for byte.
    Jpeg(Vec<u8>),
    /// Decoded samples.
    Pixels(DynamicImage),
}

#[derive(Default)]
pub struct Images {
    pub images: Vec<(usize, Extracted)>,
    /// Images that could not be decoded.
    pub skipped: usize,
}

pub(crate) fn resources<'a>(doc: &'a Document, holder: &'a Dictionary) -> Option<&'a Dictionary> {
    deref(doc, holder.get(b"Resources").ok()?).as_dict().ok()
}

/// Image objects used by a resource dictionary, looking inside form XObjects.
pub(crate) fn images_of(
    doc: &Document,
    res: &Dictionary,
    seen: &mut HashSet<ObjectId>,
    out: &mut Vec<ObjectId>,
) {
    let Some(xobjects) = res
        .get(b"XObject")
        .ok()
        .map(|o| deref(doc, o))
        .and_then(|o| o.as_dict().ok())
    else {
        return;
    };
    for (_, value) in xobjects.iter() {
        let Ok(id) = value.as_reference() else {
            continue;
        };
        if !seen.insert(id) {
            continue;
        }
        let Ok(stream) = doc.get_object(id).and_then(Object::as_stream) else {
            continue;
        };
        match stream.dict.get(b"Subtype").and_then(Object::as_name) {
            Ok(b"Image") => out.push(id),
            Ok(b"Form") => {
                if let Some(inner) = resources(doc, &stream.dict) {
                    images_of(doc, inner, seen, out);
                }
            }
            _ => {}
        }
    }
}

fn filters(doc: &Document, dict: &Dictionary) -> Vec<Vec<u8>> {
    match dict.get(b"Filter").ok().map(|o| deref(doc, o)) {
        Some(Object::Name(n)) => vec![n.clone()],
        Some(Object::Array(a)) => a
            .iter()
            .filter_map(|o| deref(doc, o).as_name().ok().map(<[u8]>::to_vec))
            .collect(),
        _ => vec![],
    }
}

#[derive(Debug)]
enum Space {
    Gray,
    Rgb,
    Cmyk,
    Indexed(Vec<u8>, usize),
    /// One ink (a Separation or a one-colorant DeviceN): the colour at no
    /// tint and at full tint.
    Tint([u8; 3], [u8; 3]),
}

/// Channels of an image a compressor can decode: gray, RGB or CMYK.
pub(crate) fn decodable_channels(doc: &Document, o: &Object) -> Option<usize> {
    match color_space(doc, o)? {
        Space::Gray => Some(1),
        Space::Rgb => Some(3),
        Space::Cmyk => Some(4),
        Space::Indexed(..) | Space::Tint(..) => None,
    }
}

fn color_space(doc: &Document, o: &Object) -> Option<Space> {
    match deref(doc, o) {
        Object::Name(n) => match n.as_slice() {
            b"DeviceGray" | b"CalGray" | b"G" => Some(Space::Gray),
            b"DeviceRGB" | b"CalRGB" | b"RGB" => Some(Space::Rgb),
            b"DeviceCMYK" | b"CMYK" => Some(Space::Cmyk),
            _ => None,
        },
        Object::Array(a) => {
            let kind = deref(doc, a.first()?).as_name().ok()?;
            match kind {
                b"ICCBased" => {
                    let profile = deref(doc, a.get(1)?).as_stream().ok()?;
                    match num(deref(doc, profile.dict.get(b"N").ok()?))? as i64 {
                        1 => Some(Space::Gray),
                        3 => Some(Space::Rgb),
                        4 => Some(Space::Cmyk),
                        _ => None,
                    }
                }
                b"CalRGB" => Some(Space::Rgb),
                b"CalGray" => Some(Space::Gray),
                b"Indexed" | b"I" => {
                    let base = color_space(doc, a.get(1)?)?;
                    let comps = match base {
                        Space::Gray => 1,
                        Space::Rgb => 3,
                        Space::Cmyk => 4,
                        Space::Indexed(..) | Space::Tint(..) => return None,
                    };
                    let table = match deref(doc, a.get(3)?) {
                        Object::String(s, _) => s.clone(),
                        Object::Stream(s) => s.decompressed_content().ok()?,
                        _ => return None,
                    };
                    Some(Space::Indexed(table, comps))
                }
                b"Separation" | b"DeviceN" => {
                    let names: Vec<&[u8]> = match deref(doc, a.get(1)?) {
                        Object::Name(n) => vec![n.as_slice()],
                        Object::Array(v) => v
                            .iter()
                            .filter_map(|o| deref(doc, o).as_name().ok())
                            .collect(),
                        _ => return None,
                    };
                    match names.as_slice() {
                        [b"Black" | b"All"] => Some(Space::Tint([255; 3], [0; 3])),
                        [_] => tint_ends(doc, a.get(2)?, a.get(3)?),
                        _ => None,
                    }
                }
                _ => None,
            }
        }
        _ => None,
    }
}

/// The colours of one ink at no and at full tint, when its tint transform is
/// an exponential function (type 2) into gray, RGB or CMYK.
fn tint_ends(doc: &Document, alternate: &Object, function: &Object) -> Option<Space> {
    let f = match deref(doc, function) {
        Object::Dictionary(d) => d,
        Object::Stream(s) => &s.dict,
        _ => return None,
    };
    if f.get(b"FunctionType").ok().and_then(num)? as i64 != 2 {
        return None;
    }
    let values = |key: &[u8], default: f64| -> Vec<f64> {
        f.get(key)
            .ok()
            .and_then(|o| deref(doc, o).as_array().ok())
            .map(|a| a.iter().filter_map(|o| num(deref(doc, o))).collect())
            .unwrap_or_else(|| vec![default])
    };
    let rgb = |v: &[f64]| -> Option<[u8; 3]> {
        let b = |x: f64| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
        match (color_space(doc, alternate)?, v) {
            (Space::Gray, [g]) => Some([b(*g); 3]),
            (Space::Rgb, [r, g, bl]) => Some([b(*r), b(*g), b(*bl)]),
            (Space::Cmyk, [c, m, y, k]) => Some(cmyk_to_rgb(b(*c), b(*m), b(*y), b(*k))),
            _ => None,
        }
    };
    Some(Space::Tint(
        rgb(&values(b"C0", 0.0))?,
        rgb(&values(b"C1", 1.0))?,
    ))
}

/// Read sample `n` of a row at `bits` per component.
fn sample(row: &[u8], n: usize, bits: usize) -> u8 {
    match bits {
        8 => row.get(n).copied().unwrap_or(0),
        16 => row.get(n * 2).copied().unwrap_or(0),
        _ => {
            let bit = n * bits;
            let byte = row.get(bit / 8).copied().unwrap_or(0);
            let shift = 8 - bits - bit % 8;
            let max = (1u16 << bits) - 1;
            (byte >> shift) & max as u8
        }
    }
}

fn cmyk_to_rgb(c: u8, m: u8, y: u8, k: u8) -> [u8; 3] {
    let f = |v: u8| (255 - v as u32) * (255 - k as u32) / 255;
    [f(c) as u8, f(m) as u8, f(y) as u8]
}

fn decode(doc: &Document, stream: &Stream) -> Option<DynamicImage> {
    let d = &stream.dict;
    let int = |k: &[u8]| {
        d.get(k)
            .ok()
            .and_then(|o| num(deref(doc, o)))
            .map(|n| n as usize)
    };
    let (w, h) = (int(b"Width")?, int(b"Height")?);
    let mask = d
        .get(b"ImageMask")
        .ok()
        .and_then(|o| deref(doc, o).as_bool().ok())
        .unwrap_or(false);
    let bits = if mask {
        1
    } else {
        int(b"BitsPerComponent").unwrap_or(8)
    };
    let space = if mask {
        Space::Gray
    } else {
        color_space(doc, d.get(b"ColorSpace").ok()?)?
    };
    let data = stream.decompressed_content().ok()?;
    let comps = match space {
        Space::Gray | Space::Indexed(..) | Space::Tint(..) => 1,
        Space::Rgb => 3,
        Space::Cmyk => 4,
    };
    let stride = (w * comps * bits).div_ceil(8);
    let scale = |v: u8| -> u8 {
        match bits {
            8 | 16 => v,
            _ => (v as u32 * 255 / ((1 << bits) - 1)) as u8,
        }
    };
    let mut rgb = Vec::with_capacity(w * h * 3);
    for y in 0..h {
        let row = data.get(y * stride..)?;
        for x in 0..w {
            match &space {
                Space::Gray => {
                    let v = scale(sample(row, x, bits));
                    rgb.extend_from_slice(&[v, v, v]);
                }
                Space::Rgb => {
                    for c in 0..3 {
                        rgb.push(scale(sample(row, x * 3 + c, bits)));
                    }
                }
                Space::Cmyk => {
                    let v: Vec<u8> = (0..4)
                        .map(|c| scale(sample(row, x * 4 + c, bits)))
                        .collect();
                    rgb.extend_from_slice(&cmyk_to_rgb(v[0], v[1], v[2], v[3]));
                }
                Space::Tint(lo, hi) => {
                    let t = scale(sample(row, x, bits)) as i32;
                    for c in 0..3 {
                        let (a, b) = (lo[c] as i32, hi[c] as i32);
                        rgb.push((a + (b - a) * t / 255) as u8);
                    }
                }
                Space::Indexed(table, n) => {
                    let i = sample(row, x, bits) as usize * n;
                    let entry = table.get(i..i + n).unwrap_or(&[0, 0, 0, 0][..*n]);
                    match n {
                        1 => rgb.extend_from_slice(&[entry[0]; 3]),
                        3 => rgb.extend_from_slice(entry),
                        _ => rgb.extend_from_slice(&cmyk_to_rgb(
                            entry[0], entry[1], entry[2], entry[3],
                        )),
                    }
                }
            }
        }
    }
    let img = RgbImage::from_raw(w as u32, h as u32, rgb)?;
    let gray = |c: &[u8; 3]| c[0] == c[1] && c[1] == c[2];
    if matches!(&space, Space::Gray)
        || matches!(&space, Space::Tint(lo, hi) if gray(lo) && gray(hi))
    {
        return Some(DynamicImage::ImageLuma8(
            DynamicImage::ImageRgb8(img).to_luma8(),
        ));
    }
    Some(with_soft_mask(doc, d, img))
}

fn with_soft_mask(doc: &Document, dict: &Dictionary, img: RgbImage) -> DynamicImage {
    let alpha: Option<GrayImage> = dict
        .get(b"SMask")
        .ok()
        .and_then(|o| o.as_reference().ok())
        .and_then(|id| doc.get_object(id).ok())
        .and_then(|o| o.as_stream().ok())
        .and_then(|s| decode(doc, s))
        .map(|m| m.to_luma8());
    match alpha {
        Some(a) if a.dimensions() == img.dimensions() => {
            let mut out = RgbaImage::new(img.width(), img.height());
            for ((o, p), m) in out.pixels_mut().zip(img.pixels()).zip(a.pixels()) {
                o.0 = [p.0[0], p.0[1], p.0[2], m.0[0]];
            }
            DynamicImage::ImageRgba8(out)
        }
        _ => DynamicImage::ImageRgb8(img),
    }
}

/// Whether a JPEG from the PDF looks the same saved on its own. A CMYK JPEG
/// does not: viewers read an Adobe CMYK JPEG as inverted ink, which a PDF
/// does not, so it would show as a negative. Nor does one the PDF inverts
/// with /Decode. Those are decoded as the page draws them instead.
fn shows_as_drawn(doc: &Document, dict: &Dictionary, jpeg: &[u8]) -> bool {
    let decoded_as_is = dict
        .get(b"Decode")
        .ok()
        .and_then(|o| deref(doc, o).as_array().ok())
        .and_then(|a| a.first().and_then(|v| num(deref(doc, v))))
        .is_none_or(|first| first < 0.5);
    decoded_as_is && jpeg_components(jpeg).is_some_and(|n| n != 4)
}

/// The number of colour components a JPEG's frame header names.
fn jpeg_components(jpeg: &[u8]) -> Option<u8> {
    let mut i = 2;
    while i + 4 <= jpeg.len() {
        if jpeg[i] != 0xFF {
            return None;
        }
        let marker = jpeg[i + 1];
        if marker == 0xFF {
            i += 1;
            continue;
        }
        let len = u16::from_be_bytes([jpeg[i + 2], jpeg[i + 3]]) as usize;
        // Start of frame: SOF0-SOF15 except DHT, JPG and DAC.
        if (0xC0..=0xCF).contains(&marker) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            return jpeg.get(i + 9).copied();
        }
        i += 2 + len;
    }
    None
}

/// A JPEG packed inside other filters, such as [/FlateDecode /DCTDecode]:
/// the JPEG bytes with the outer filters undone.
fn wrapped_jpeg(stream: &Stream, chain: &[Vec<u8>]) -> Option<Vec<u8>> {
    use std::io::Read;
    let (last, outer) = chain.split_last()?;
    if last != b"DCTDecode" || outer.iter().any(|f| f != b"FlateDecode") {
        return None;
    }
    if stream.dict.has(b"DecodeParms") {
        let parms = stream.dict.get(b"DecodeParms").ok()?;
        let plain = match parms {
            Object::Array(a) => a.iter().all(|p| matches!(p, Object::Null)),
            _ => false,
        };
        if !plain {
            return None;
        }
    }
    let mut data = stream.content.clone();
    for _ in outer {
        let mut out = Vec::new();
        flate2::read::ZlibDecoder::new(data.as_slice())
            .read_to_end(&mut out)
            .ok()?;
        data = out;
    }
    data.starts_with(&[0xFF, 0xD8]).then_some(data)
}

/// Images in the scan encodings (CCITT fax, JBIG2, JPEG 2000), decoded with
/// the renderer's own codecs. `pdf` holds the document with the same object
/// numbers.
fn decode_scan(
    pdf: &hayro::hayro_syntax::Pdf,
    doc: &Document,
    id: ObjectId,
) -> Option<DynamicImage> {
    use hayro::hayro_syntax::object::ObjectIdentifier;
    use hayro::hayro_syntax::object::stream::{ImageColorSpace, ImageDecodeParams};
    let stream = doc.get_object(id).ok()?.as_stream().ok()?;
    let d = &stream.dict;
    let int = |k: &[u8]| {
        d.get(k)
            .ok()
            .and_then(|o| num(deref(doc, o)))
            .map(|n| n as u32)
    };
    let mask = d
        .get(b"ImageMask")
        .ok()
        .and_then(|o| deref(doc, o).as_bool().ok())
        .unwrap_or(false);
    let space = match d.get(b"ColorSpace") {
        Ok(o) => color_space(doc, o),
        Err(_) => Some(Space::Gray),
    };
    let comps = match space {
        Some(Space::Rgb) => 3,
        Some(Space::Cmyk) => 4,
        _ => 1,
    };
    let params = ImageDecodeParams {
        is_indexed: matches!(space, Some(Space::Indexed(..))),
        bpc: int(b"BitsPerComponent").map(|b| b as u8),
        num_components: Some(comps),
        target_dimension: None,
        width: int(b"Width")?,
        height: int(b"Height")?,
    };
    let found: hayro::hayro_syntax::object::Stream<'_> = pdf
        .xref()
        .get(ObjectIdentifier::new(id.0 as i32, id.1 as i32))?;
    let decoded = found.decoded_image(&params).ok()?;
    let (w, h, bpc, cs) = match &decoded.image_data {
        Some(i) => (i.width, i.height, i.bits_per_component, i.color_space),
        None => (params.width, params.height, params.bpc.unwrap_or(8), None),
    };
    let channels = match cs {
        Some(ImageColorSpace::Rgb) => 3,
        Some(ImageColorSpace::Cmyk) => 4,
        Some(ImageColorSpace::Gray) => 1,
        _ => comps as usize,
    };
    let data = decoded.data.as_ref();
    let samples: Vec<u8> = match bpc {
        8 => data.to_vec(),
        1 if channels == 1 => {
            let stride = (w as usize).div_ceil(8);
            (0..h as usize)
                .flat_map(|y| (0..w as usize).map(move |x| (y, x)))
                .map(|(y, x)| {
                    let byte = data.get(y * stride + x / 8).copied().unwrap_or(0xFF);
                    if byte & (0x80 >> (x % 8)) != 0 {
                        255
                    } else {
                        0
                    }
                })
                .collect()
        }
        _ => return None,
    };
    if samples.len() < (w * h) as usize * channels {
        return None;
    }
    // /Decode [1 0 ...] inverts the samples; a stencil mask paints its 0s.
    let inverted = d
        .get(b"Decode")
        .ok()
        .and_then(|o| deref(doc, o).as_array().ok())
        .and_then(|a| a.first().and_then(|v| num(deref(doc, v))))
        .is_some_and(|first| first > 0.5);
    let samples = &samples[..(w * h) as usize * channels];
    let img = match channels {
        1 => {
            let gray: Vec<u8> = if inverted && !mask {
                samples.iter().map(|v| 255 - v).collect()
            } else {
                samples.to_vec()
            };
            DynamicImage::ImageLuma8(GrayImage::from_raw(w, h, gray)?)
        }
        3 => {
            let rgb: Vec<u8> = if inverted {
                samples.iter().map(|v| 255 - v).collect()
            } else {
                samples.to_vec()
            };
            DynamicImage::ImageRgb8(RgbImage::from_raw(w, h, rgb)?)
        }
        4 => {
            let ink = |v: u8| if inverted { 255 - v } else { v };
            let rgb: Vec<u8> = samples
                .chunks_exact(4)
                .flat_map(|p| cmyk_to_rgb(ink(p[0]), ink(p[1]), ink(p[2]), ink(p[3])))
                .collect();
            DynamicImage::ImageRgb8(RgbImage::from_raw(w, h, rgb)?)
        }
        _ => return None,
    };
    let alpha = decoded.image_data.as_ref().and_then(|i| i.alpha.clone());
    Some(match alpha {
        Some(a) if a.len() == (w * h) as usize => {
            let mut out = img.to_rgba8();
            for (p, a) in out.pixels_mut().zip(a) {
                p.0[3] = a;
            }
            DynamicImage::ImageRgba8(out)
        }
        _ => img,
    })
}

/// Images on the given pages (zero-based), each with its page.
pub fn images(doc: &Document, indexes: &[usize]) -> Images {
    let pages = crate::page_ids(doc);
    let mut out = Images::default();
    let mut seen = HashSet::new();
    // The renderer's parse of the document, made when a scan image needs it.
    let mut renderer: Option<Option<hayro::hayro_syntax::Pdf>> = None;
    for &i in indexes {
        let Some(page) = pages.get(i) else {
            continue;
        };
        let Ok(dict) = doc.get_dictionary(*page) else {
            continue;
        };
        let mut ids = Vec::new();
        if let Some(res) = resources(doc, dict) {
            images_of(doc, res, &mut seen, &mut ids);
        }
        for id in ids {
            let Ok(stream) = doc.get_object(id).and_then(Object::as_stream) else {
                continue;
            };
            let chain = filters(doc, &stream.dict);
            let last = chain.last().map(Vec::as_slice);
            let jpeg = if chain.len() == 1 && last == Some(b"DCTDecode") {
                Some(stream.content.clone())
            } else {
                wrapped_jpeg(stream, &chain)
            };
            if let Some(jpeg) = jpeg.filter(|j| shows_as_drawn(doc, &stream.dict, j)) {
                out.images.push((i, Extracted::Jpeg(jpeg)));
            } else if matches!(
                last,
                Some(b"JBIG2Decode" | b"JPXDecode" | b"CCITTFaxDecode" | b"DCTDecode")
            ) {
                let pdf = renderer.get_or_insert_with(|| {
                    let mut copy = doc.clone();
                    copy.encryption_state = None;
                    copy.trailer.remove(b"Encrypt");
                    let mut bytes = Vec::new();
                    copy.save_to(&mut bytes).ok()?;
                    hayro::hayro_syntax::Pdf::new(bytes).ok()
                });
                match pdf.as_ref().and_then(|pdf| decode_scan(pdf, doc, id)) {
                    Some(img) => out.images.push((i, Extracted::Pixels(img))),
                    None => out.skipped += 1,
                }
            } else if let Some(img) = decode(doc, stream) {
                out.images.push((i, Extracted::Pixels(img)));
            } else {
                out.skipped += 1;
            }
        }
    }
    out
}

#[cfg(test)]
#[path = "extract_tests.rs"]
mod tests;
