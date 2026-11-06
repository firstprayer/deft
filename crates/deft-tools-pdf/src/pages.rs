//! Changes to the page geometry and to what is drawn on pages.

use crate::{Renderer, deref, display_matrix, display_size, page_box, rect_of};
use deft_core::error::{ErrorCode, Result, err};
use lopdf::{Dictionary, Document, Object, ObjectId, Stream, dictionary};

/// Apply a transform to a point.
fn apply(m: [f64; 6], x: f64, y: f64) -> (f64, f64) {
    (m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5])
}

fn set_box(doc: &mut Document, page: ObjectId, b: [f64; 4]) -> Result<()> {
    let rect = || Object::Array(b.iter().map(|v| Object::Real(*v as f32)).collect());
    let d = doc
        .get_dictionary_mut(page)
        .map_err(|e| err(ErrorCode::InputCorrupt, e.to_string()))?;
    d.set("MediaBox", rect());
    d.set("CropBox", rect());
    for key in [&b"TrimBox"[..], b"BleedBox", b"ArtBox"] {
        d.remove(key);
    }
    Ok(())
}

/// Cut `margin` points off every side of a page.
pub fn crop_margin(doc: &mut Document, page: ObjectId, margin: f64) -> Result<()> {
    let [x0, y0, x1, y1] = page_box(doc, page);
    if x1 - x0 <= 2.0 * margin || y1 - y0 <= 2.0 * margin {
        return Err(err(
            ErrorCode::BadValue,
            "the margin is larger than the page",
        ));
    }
    set_box(
        doc,
        page,
        [x0 + margin, y0 + margin, x1 - margin, y1 - margin],
    )
}

/// Crop to a region measured in points from the top-left of the page as shown.
pub fn crop_region(doc: &mut Document, page: ObjectId, region: [f64; 4]) -> Result<()> {
    let [x, y, w, h] = region;
    let (pw, ph) = display_size(doc, page);
    if w <= 0.0 || h <= 0.0 || x < 0.0 || y < 0.0 || x + w > pw + 0.5 || y + h > ph + 0.5 {
        return Err(err(
            ErrorCode::BadValue,
            format!("region is outside the {pw:.0}x{ph:.0}pt page"),
        ));
    }
    let m = display_matrix(doc, page);
    let (ax, ay) = apply(m, x, ph - y - h);
    let (bx, by) = apply(m, x + w, ph - y);
    set_box(doc, page, [ax.min(bx), ay.min(by), ax.max(bx), ay.max(by)])
}

/// Where non-white content sits on a rendered page, as a region in points from
/// the top-left. `None` for a blank page.
pub fn content_region(r: &Renderer, index: usize, max_pixels: u64) -> Result<Option<[f64; 4]>> {
    let dpi = 72.0;
    let img = r.render(index, dpi, max_pixels)?;
    let (w, h) = img.dimensions();
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for (x, y, p) in img.enumerate_pixels() {
        if p.0.iter().any(|v| *v < 245) {
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    Ok((x0 <= x1).then(|| {
        [
            x0 as f64,
            y0 as f64,
            (x1 + 1 - x0) as f64,
            (y1 + 1 - y0) as f64,
        ]
    }))
}

/// Scale a page's content onto a new paper size, centred.
pub fn resize_page(doc: &mut Document, page: ObjectId, w: f64, h: f64) -> Result<()> {
    let [x0, y0, x1, y1] = page_box(doc, page);
    let (cw, ch) = (x1 - x0, y1 - y0);
    let rotated = crate::page_rotation(doc, page) % 180 == 90;
    let (w, h) = if rotated { (h, w) } else { (w, h) };
    let s = (w / cw).min(h / ch);
    let (dx, dy) = ((w - cw * s) / 2.0 - x0 * s, (h - ch * s) / 2.0 - y0 * s);
    let pre = format!("q {s:.5} 0 0 {s:.5} {dx:.3} {dy:.3} cm\n").into_bytes();
    wrap_contents(doc, page, pre, b"Q\n".to_vec())?;
    set_box(doc, page, [0.0, 0.0, w, h])
}

fn wrap_contents(doc: &mut Document, page: ObjectId, pre: Vec<u8>, post: Vec<u8>) -> Result<()> {
    let old = current_contents(doc, page);
    let a = doc.add_object(Stream::new(Dictionary::new(), pre));
    let b = doc.add_object(Stream::new(Dictionary::new(), post));
    let mut contents = vec![Object::Reference(a)];
    contents.extend(old);
    contents.push(Object::Reference(b));
    doc.get_dictionary_mut(page)
        .map_err(|e| err(ErrorCode::InputCorrupt, e.to_string()))?
        .set("Contents", Object::Array(contents));
    Ok(())
}

fn current_contents(doc: &Document, page: ObjectId) -> Vec<Object> {
    match doc
        .get_dictionary(page)
        .ok()
        .and_then(|d| d.get(b"Contents").ok())
    {
        Some(Object::Reference(id)) => match doc.get_object(*id) {
            Ok(Object::Array(a)) => a.clone(),
            _ => vec![Object::Reference(*id)],
        },
        Some(Object::Array(a)) => a.clone(),
        _ => vec![],
    }
}

/// Draw form XObjects on a page. Each entry is (name, object, `cm` matrix).
pub(crate) fn draw_forms(
    doc: &mut Document,
    page: ObjectId,
    forms: &[(String, ObjectId, [f64; 6])],
) -> Result<()> {
    if forms.is_empty() {
        return Ok(());
    }
    let mut ops = String::from("Q\n");
    for (name, _, m) in forms {
        ops.push_str(&format!(
            "q {} {} {} {} {} {} cm /{name} Do Q\n",
            m[0], m[1], m[2], m[3], m[4], m[5]
        ));
    }
    let mut res = doc
        .get_dictionary(page)
        .ok()
        .and_then(|d| d.get(b"Resources").ok())
        .map(|o| deref(doc, o))
        .and_then(|o| o.as_dict().ok())
        .cloned()
        .unwrap_or_default();
    let mut xobjects = res
        .get(b"XObject")
        .ok()
        .map(|o| deref(doc, o))
        .and_then(|o| o.as_dict().ok())
        .cloned()
        .unwrap_or_default();
    for (name, id, _) in forms {
        xobjects.set(name.as_bytes().to_vec(), Object::Reference(*id));
    }
    res.set("XObject", Object::Dictionary(xobjects));
    wrap_contents(doc, page, b"q\n".to_vec(), ops.into_bytes())?;
    doc.get_dictionary_mut(page)
        .map_err(|e| err(ErrorCode::InputCorrupt, e.to_string()))?
        .set("Resources", Object::Dictionary(res));
    Ok(())
}

/// Put the invisible text of an OCR page over a page of `doc`. The text page
/// must already be part of `doc`. The text goes straight into the page's
/// content, not a form XObject, because many extractors skip those.
pub fn overlay_page(doc: &mut Document, page: ObjectId, text_page: ObjectId) -> Result<()> {
    let content = doc.get_page_content(text_page);
    let (tw, th) = display_size(doc, text_page);
    let (pw, ph) = display_size(doc, page);
    let m = display_matrix(doc, page);
    let (sx, sy) = (pw / tw, ph / th);
    let resources = |doc: &Document, id: ObjectId| {
        doc.get_dictionary(id)
            .ok()
            .and_then(|d| d.get(b"Resources").ok())
            .map(|o| deref(doc, o))
            .and_then(|o| o.as_dict().ok())
            .cloned()
            .unwrap_or_default()
    };
    let fonts = |doc: &Document, res: &Dictionary| {
        res.get(b"Font")
            .ok()
            .map(|o| deref(doc, o))
            .and_then(|o| o.as_dict().ok())
            .cloned()
            .unwrap_or_default()
    };
    let text_res = resources(doc, text_page);
    let mut res = resources(doc, page);
    let mut merged = fonts(doc, &res);
    for (name, value) in fonts(doc, &text_res).iter() {
        merged.set(name.clone(), value.clone());
    }
    res.set("Font", Object::Dictionary(merged));
    // The layer's coordinates, scaled to the page as shown, then to user space.
    let mut post = format!(
        "Q\nq {} {} {} {} {} {} cm\n",
        m[0] * sx,
        m[1] * sx,
        m[2] * sy,
        m[3] * sy,
        m[4],
        m[5]
    )
    .into_bytes();
    post.extend_from_slice(&content);
    post.extend_from_slice(b"\nQ\n");
    wrap_contents(doc, page, b"q\n".to_vec(), post)?;
    doc.get_dictionary_mut(page)
        .map_err(|e| err(ErrorCode::InputCorrupt, e.to_string()))?
        .set("Resources", Object::Dictionary(res));
    Ok(())
}

/// Black out regions by replacing the page with a picture of itself, so the
/// text under the boxes is really gone. Regions are in points from the
/// top-left of the page as shown. Returns the picture to embed.
pub fn redacted_image(
    r: &Renderer,
    index: usize,
    regions: &[[f64; 4]],
    dpi: f64,
    max_pixels: u64,
) -> Result<image::RgbImage> {
    let mut img = r.render(index, dpi, max_pixels)?;
    let k = dpi / 72.0;
    let (w, h) = img.dimensions();
    for [x, y, rw, rh] in regions {
        let (x0, y0) = ((x * k) as u32, (y * k) as u32);
        let (x1, y1) = (
            (((x + rw) * k).ceil() as u32).min(w),
            (((y + rh) * k).ceil() as u32).min(h),
        );
        for py in y0..y1 {
            for px in x0..x1 {
                img.put_pixel(px, py, image::Rgb([0, 0, 0]));
            }
        }
    }
    Ok(img)
}

/// Replace a page's content with one full-page image, dropping its old
/// resources, annotations and rotation.
pub fn replace_with_image(
    doc: &mut Document,
    page: ObjectId,
    img: &crate::PdfImage,
    w: f64,
    h: f64,
) -> Result<()> {
    let id = crate::embed_image(doc, img);
    let content = format!("q {w:.3} 0 0 {h:.3} 0 0 cm /DeftPage Do Q\n").into_bytes();
    let stream = doc.add_object(Stream::new(Dictionary::new(), content));
    let resources = dictionary! { "XObject" => dictionary! { "DeftPage" => id } };
    let d = doc
        .get_dictionary_mut(page)
        .map_err(|e| err(ErrorCode::InputCorrupt, e.to_string()))?;
    d.set("Contents", Object::Reference(stream));
    d.set("Resources", Object::Dictionary(resources));
    d.remove(b"Annots");
    d.remove(b"Rotate");
    set_box(doc, page, [0.0, 0.0, w, h])
}

/// The rectangle of an annotation.
pub(crate) fn annot_rect(doc: &Document, annot: &Dictionary) -> Option<[f64; 4]> {
    rect_of(doc, annot.get(b"Rect").ok()?)
}
