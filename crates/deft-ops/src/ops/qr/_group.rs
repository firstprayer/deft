use deft_core::prelude::*;

pub static GROUP: Group = Group {
    name: "qr",
    summary: "Create QR codes and barcodes; read QR codes",
    aliases: &["barcode", "qrcode"],
    synonyms: &[
        ("generate", "create"),
        ("make", "create"),
        ("scan", "read"),
        ("decode", "read"),
    ],
    order: 80,
};
