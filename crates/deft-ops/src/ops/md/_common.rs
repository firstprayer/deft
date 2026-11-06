//! Typesetting shared by every operation that ends in a PDF through Markdown.

use deft_core::prelude::*;
use deft_tools_doc::Image;
use deft_tools_text::PdfStyle;
use std::path::Path;

pub const PAGE_SIZE: ParamSpec = param(
    "page-size",
    Kind::Choice(&["a4", "letter", "legal", "a3", "a5"]),
    "Paper",
)
.default("a4");
pub const MARGIN: ParamSpec = param("margin", Kind::Length, "Page margin").default("25mm");
pub const FONT_SIZE: ParamSpec = param(
    "font-size",
    Kind::Float {
        min: 4.0,
        max: 72.0,
    },
    "Body text size in points",
)
.default("11");
pub const FONT: ParamSpec = param("font", Kind::Text, "Font family installed on this machine");

/// Typeset Markdown with the options above. `base` resolves relative images.
pub fn markdown_pdf(job: &Job, md: &str, base: Option<&Path>) -> Result<OpResult> {
    let p = job.params;
    let paper = match p.text("page-size").as_str() {
        "letter" => "us-letter".to_string(),
        "legal" => "us-legal".to_string(),
        other => other.to_string(),
    };
    let style = PdfStyle {
        paper,
        margin: p.points("margin"),
        font_size: p.float("font-size"),
        font: p.opt_text("font"),
    };
    let out = deft_tools_text::to_pdf(md, base, &style)?;
    let mut res = OpResult::one(Produced::bytes(out.bytes, Format::Pdf))
        .fact("pages", out.pages)
        .note(format!(
            "{} page{}",
            out.pages,
            if out.pages == 1 { "" } else { "s" }
        ));
    res.warnings = out.warnings;
    Ok(res)
}

/// Write images to a scratch folder for the typesetter. Formats it cannot
/// embed, such as EMF, are left for it to warn about.
pub fn images_folder(job: &Job, images: &[Image]) -> Result<std::path::PathBuf> {
    let dir = job.ctx.temp_path("images");
    std::fs::create_dir_all(&dir)?;
    for image in images {
        std::fs::write(dir.join(&image.name), &image.bytes)?;
    }
    Ok(dir)
}

/// Save images next to a Markdown file when `--images <dir>` asks for it.
pub fn save_images(images: &[Image], dir: Option<std::path::PathBuf>) -> Result<Option<String>> {
    if images.is_empty() {
        return Ok(None);
    }
    let Some(dir) = dir else {
        return Ok(Some(format!(
            "{} images were not saved; add --images <dir>",
            images.len()
        )));
    };
    std::fs::create_dir_all(&dir)?;
    for image in images {
        std::fs::write(dir.join(&image.name), &image.bytes)?;
    }
    Ok(None)
}

/// Read the images a Markdown document refers to, relative to `base`.
pub fn load_images(blocks: &mut [deft_tools_doc::Block], base: Option<&Path>) -> Vec<Image> {
    use deft_tools_doc::{Block, Span};
    fn spans(list: &mut [Span], base: Option<&Path>, out: &mut Vec<Image>) {
        for s in list {
            let Some(path) = s.image.clone() else {
                continue;
            };
            let full = base.map_or_else(|| Path::new(&path).to_path_buf(), |b| b.join(&path));
            let name = Path::new(&path)
                .file_name()
                .map_or(path.clone(), |n| n.to_string_lossy().to_string());
            match std::fs::read(&full) {
                Ok(bytes) => {
                    if !out.iter().any(|i| i.name == name) {
                        out.push(Image {
                            name: name.clone(),
                            bytes,
                        });
                    }
                    s.image = Some(name);
                }
                // A web image or a missing file: keep its text.
                Err(_) => {
                    s.image = None;
                    if s.text.is_empty() {
                        s.text = path;
                    }
                }
            }
        }
    }
    fn walk(blocks: &mut [Block], base: Option<&Path>, out: &mut Vec<Image>) {
        for b in blocks {
            match b {
                Block::Heading(_, s) | Block::Paragraph(s) => spans(s, base, out),
                Block::List { items, .. } => {
                    for item in items {
                        walk(item, base, out);
                    }
                }
                Block::Quote(inner) => walk(inner, base, out),
                Block::Table(rows) => {
                    for row in rows {
                        for cell in row {
                            spans(cell, base, out);
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    walk(blocks, base, &mut out);
    out
}
