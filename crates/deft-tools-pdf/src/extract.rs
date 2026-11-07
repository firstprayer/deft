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
    /// Images in encodings deft cannot unpack, such as JBIG2.
    pub skipped: usize,
}

fn resources<'a>(doc: &'a Document, holder: &'a Dictionary) -> Option<&'a Dictionary> {
    deref(doc, holder.get(b"Resources").ok()?).as_dict().ok()
}

/// Image objects used by a resource dictionary, looking inside form XObjects.
fn images_of(
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
}

/// Channels of an image a compressor can decode: gray, RGB or CMYK.
pub(crate) fn decodable_channels(doc: &Document, o: &Object) -> Option<usize> {
    match color_space(doc, o)? {
        Space::Gray => Some(1),
        Space::Rgb => Some(3),
        Space::Cmyk => Some(4),
        Space::Indexed(..) => None,
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
                        Space::Indexed(..) => return None,
                    };
                    let table = match deref(doc, a.get(3)?) {
                        Object::String(s, _) => s.clone(),
                        Object::Stream(s) => s.decompressed_content().ok()?,
                        _ => return None,
                    };
                    Some(Space::Indexed(table, comps))
                }
                _ => None,
            }
        }
        _ => None,
    }
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
        Space::Gray | Space::Indexed(..) => 1,
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
    if matches!(space, Space::Gray) {
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

/// Images on the given pages (zero-based), each with its page.
pub fn images(doc: &Document, indexes: &[usize]) -> Images {
    let pages = crate::page_ids(doc);
    let mut out = Images::default();
    let mut seen = HashSet::new();
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
            if chain.len() == 1 && last == Some(b"DCTDecode") {
                out.images
                    .push((i, Extracted::Jpeg(stream.content.clone())));
            } else if matches!(
                last,
                Some(b"JBIG2Decode" | b"JPXDecode" | b"CCITTFaxDecode" | b"DCTDecode")
            ) {
                out.skipped += 1;
            } else if let Some(img) = decode(doc, stream) {
                out.images.push((i, Extracted::Pixels(img)));
            } else {
                out.skipped += 1;
            }
        }
    }
    out
}
