//! Comparing two PDFs by text and by rendered pixels.

use crate::Renderer;
use deft_core::error::Result;
use lopdf::Document;
use serde_json::{Value, json};

#[derive(Clone, Debug, Default)]
pub struct PageChange {
    pub page: usize,
    pub added: Vec<String>,
    pub removed: Vec<String>,
    /// Share of pixels that differ, when both pages were rendered.
    pub pixels: Option<f64>,
}

impl PageChange {
    pub fn to_json(&self) -> Value {
        json!({"page": self.page, "added": self.added, "removed": self.removed, "pixels": self.pixels})
    }
    pub fn differs(&self) -> bool {
        !self.added.is_empty() || !self.removed.is_empty() || self.pixels.is_some_and(|p| p > 0.001)
    }
}

fn page_texts(doc: &Document, pages: usize) -> Result<Vec<String>> {
    let indexes: Vec<usize> = (0..pages).collect();
    Ok(crate::text::reader(doc)?.page_texts(&indexes))
}

/// Compare page by page. With `visual`, pages are rendered at `dpi` too.
pub fn compare(
    a: &Document,
    b: &Document,
    a_render: Option<&Renderer>,
    b_render: Option<&Renderer>,
    dpi: f64,
    max_pixels: u64,
) -> Result<Vec<PageChange>> {
    let (na, nb) = (crate::page_ids(a).len(), crate::page_ids(b).len());
    let (ta, tb) = (page_texts(a, na)?, page_texts(b, nb)?);
    let mut out = Vec::new();
    for i in 0..na.max(nb) {
        let mut change = PageChange {
            page: i + 1,
            ..Default::default()
        };
        let blank = String::new();
        let (x, y) = (ta.get(i).unwrap_or(&blank), tb.get(i).unwrap_or(&blank));
        for d in similar::TextDiff::from_lines(x, y).iter_all_changes() {
            let text = d.value().trim().to_string();
            if text.is_empty() {
                continue;
            }
            match d.tag() {
                similar::ChangeTag::Delete => change.removed.push(text),
                similar::ChangeTag::Insert => change.added.push(text),
                similar::ChangeTag::Equal => {}
            }
        }
        if let (Some(ra), Some(rb)) = (a_render, b_render)
            && i < na
            && i < nb
        {
            let p = image::DynamicImage::ImageRgb8(ra.render(i, dpi, max_pixels)?);
            let q = image::DynamicImage::ImageRgb8(rb.render(i, dpi, max_pixels)?);
            change.pixels = Some(
                deft_tools_image::edit::compare(&p, &q, 5.0)
                    .map(|d| d.changed)
                    .unwrap_or(1.0),
            );
        }
        if change.differs() {
            out.push(change);
        }
    }
    Ok(out)
}
