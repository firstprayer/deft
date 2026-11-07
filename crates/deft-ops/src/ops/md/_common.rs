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

/// The folder of saved pictures as a Markdown file written by `job` must
/// name it: relative to the folder the Markdown goes to, with `/`.
pub fn folder_link(job: &Job, dir: &Path) -> String {
    use std::path::Component;
    let cwd = std::env::current_dir().unwrap_or_default();
    let absolute = |p: &Path| {
        let mut out = std::path::PathBuf::new();
        for c in cwd.join(p).components() {
            match c {
                Component::ParentDir => {
                    out.pop();
                }
                Component::CurDir => {}
                other => out.push(other),
            }
        }
        out
    };
    let Some(from) = &job.out_dir else {
        return dir.display().to_string().replace('\\', "/");
    };
    relative_link(&absolute(from), &absolute(dir))
}

/// The path from folder `from` to `to`, both absolute, with `/`.
fn relative_link(from: &Path, to: &Path) -> String {
    let common = from
        .components()
        .zip(to.components())
        .take_while(|(a, b)| a == b)
        .count();
    // Different drives have nothing in common: keep the full path.
    if common == 0 {
        return to.display().to_string().replace('\\', "/");
    }
    let mut parts: Vec<String> = vec!["..".to_string(); from.components().count() - common];
    parts.extend(
        to.components()
            .skip(common)
            .map(|c| c.as_os_str().to_string_lossy().to_string()),
    );
    parts.join("/")
}

/// Point picture links at `folder`, the result of `folder_link`.
pub fn relink_images(md: &str, images: &[Image], folder: &str) -> String {
    let mut md = md.to_string();
    let prefix = if folder.is_empty() {
        String::new()
    } else {
        format!("{folder}/")
    };
    for image in images {
        let name = deft_tools_doc::markdown::link_name(&image.name);
        for end in [")", " \""] {
            md = md.replace(&format!("]({name}{end}"), &format!("]({prefix}{name}{end}"));
        }
    }
    md
}

/// The images found so far, and what could not be used.
#[derive(Default)]
struct Loaded {
    images: Vec<Image>,
    warnings: Vec<String>,
}

impl Loaded {
    /// The name this picture goes by. Two different pictures can share a file
    /// name when they come from different folders.
    fn add(&mut self, name: &str, bytes: Vec<u8>) -> String {
        let (stem, extension) = name.rsplit_once('.').unwrap_or((name, ""));
        let mut candidate = name.to_string();
        let mut n = 1;
        loop {
            match self.images.iter().find(|i| i.name == candidate) {
                Some(i) if i.bytes == bytes => return candidate,
                Some(_) => {
                    n += 1;
                    candidate = format!("{stem}-{n}.{extension}");
                }
                None => break,
            }
        }
        self.images.push(Image {
            name: candidate.clone(),
            bytes,
        });
        candidate
    }
}

/// Read the images a Markdown document refers to, relative to `base`. Returns
/// them with a warning for each one that could not be embedded.
pub fn load_images(
    blocks: &mut [deft_tools_doc::Block],
    base: Option<&Path>,
) -> (Vec<Image>, Vec<String>) {
    use deft_tools_doc::{Block, Span};
    fn spans(list: &mut [Span], base: Option<&Path>, out: &mut Loaded) {
        for s in list {
            let Some(path) = s.image.clone() else {
                continue;
            };
            let located = |p: &str| base.map_or_else(|| Path::new(p).to_path_buf(), |b| b.join(p));
            // Markdown writes a space in a file name as %20.
            let decoded = percent_decode(&path);
            let full = [located(&path), located(&decoded)]
                .into_iter()
                .find(|p| p.is_file());
            match full.and_then(|p| Some((std::fs::read(&p).ok()?, p))) {
                Some((bytes, full)) => {
                    let name = full
                        .file_name()
                        .map_or(path.clone(), |n| n.to_string_lossy().to_string());
                    s.image = Some(out.add(&name, bytes));
                }
                // A web image or a missing file: keep its text.
                None => {
                    let remote = path.starts_with("http://") || path.starts_with("https://");
                    out.warnings.push(if remote {
                        format!("remote image not embedded: {path}")
                    } else {
                        format!("image not found: {path}")
                    });
                    s.image = None;
                    if s.text.is_empty() {
                        s.text = path;
                    }
                }
            }
        }
    }
    fn walk(blocks: &mut [Block], base: Option<&Path>, out: &mut Loaded) {
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
    let mut out = Loaded::default();
    walk(blocks, base, &mut out);
    (out.images, out.warnings)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picture_links_are_relative_to_the_markdown_file() {
        let root = if cfg!(windows) { "C:\\w" } else { "/w" };
        let at = |p: &str| Path::new(root).join(p);
        assert_eq!(relative_link(&at("sub"), &at("media")), "../media");
        assert_eq!(relative_link(&at(""), &at("media")), "media");
        assert_eq!(relative_link(&at("sub"), &at("sub/pics")), "pics");
        assert_eq!(relative_link(&at("a/b"), &at("a/b")), "");
        let images = [Image {
            name: "my cover.png".into(),
            bytes: vec![],
        }];
        let md = "![](my%20cover.png) ![Cover](my%20cover.png \"Cover\")";
        assert_eq!(
            relink_images(md, &images, "../media"),
            "![](../media/my%20cover.png) ![Cover](../media/my%20cover.png \"Cover\")"
        );
        assert_eq!(relink_images(md, &images, ""), md);
    }
}
