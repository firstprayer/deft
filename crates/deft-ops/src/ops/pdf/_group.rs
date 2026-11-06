use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "pdf",
    summary: "Merge, split, compress, encrypt, forms, OCR, compare, convert",
    aliases: &[],
    synonyms: &[
        ("stamp", "watermark"),
        ("paginate", "number"),
        ("images", "to-images"),
        ("text", "to-text"),
        ("lock", "encrypt"),
        ("protect", "encrypt"),
        ("password", "encrypt"),
        ("unlock", "decrypt"),
        ("metadata", "meta"),
        ("form", "fields"),
        ("form-fields", "fields"),
        ("fill-form", "fill"),
        ("diff", "compare"),
        ("find", "search"),
        ("grep", "search"),
        ("word", "to-docx"),
        ("docx", "to-docx"),
        ("markdown", "to-md"),
        ("scale", "resize"),
    ],
    order: 10,
};
