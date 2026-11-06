//! Helpers shared by the pdf operations.

use deft_core::prelude::*;
use deft_tools_pdf::lopdf::Document;

pub const PASSWORD: ParamSpec = param("password", Kind::Text, "Password of an encrypted input");

pub fn open(input: &InputFile, params: &Params) -> Result<Document> {
    deft_tools_pdf::load(input.bytes()?, params.opt_text("password").as_deref()).map_err(|mut e| {
        if e.code == ErrorCode::InputEncrypted || e.code == ErrorCode::InputCorrupt {
            e.message = format!("{}: {}", input.display, e.message);
        }
        e
    })
}

pub fn resolve(pages: &PageSet, count: usize) -> Result<Vec<usize>> {
    pages.resolve(count).map_err(|m| bad_value("pages", m))
}

pub fn resolve_set(pages: &PageSet, count: usize) -> Result<Vec<usize>> {
    pages.resolve_set(count).map_err(|m| bad_value("pages", m))
}

pub fn write(doc: &mut Document) -> Result<Produced> {
    Ok(Produced::bytes(deft_tools_pdf::save(doc)?, Format::Pdf))
}

pub fn plural(n: usize, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}

/// Text as a transparent image, for scripts the built-in font cannot show.
/// Returns the image with its width and height per unit of font size.
pub fn text_image(
    text: &str,
    color: [u8; 4],
    font: Option<String>,
) -> Result<(deft_tools_pdf::PdfImage, f64, f64)> {
    let px = 200.0;
    let style = deft_tools_image::TextStyle {
        font,
        ..deft_tools_image::TextStyle::new(px, [color[0], color[1], color[2], 255])
    };
    let img = deft_tools_image::render_text(text, &style)?;
    let (w, h) = (
        img.width() as f64 / px as f64,
        img.height() as f64 / px as f64,
    );
    Ok((
        deft_tools_pdf::PdfImage::from_image(&deft_tools_image::image::DynamicImage::ImageRgba8(
            img,
        )),
        w,
        h,
    ))
}
