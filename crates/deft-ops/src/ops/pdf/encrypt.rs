use super::_common::{PASSWORD, open};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "pdf.encrypt",
    summary: "Protect with a password (AES-256)",
    details: &["--new-password opens the file. --password is for an already locked input."],
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::one(&[Format::Pdf], "pdf"),
    params: &[
        param("new-password", Kind::Text, "Password to open the output").required(),
        param(
            "owner-password",
            Kind::Text,
            "Password for full control (default: the same)",
        ),
        param("no-print", Kind::Flag, "Forbid printing"),
        param("no-copy", Kind::Flag, "Forbid copying text"),
        PASSWORD,
    ],
    examples: &["deft pdf encrypt report.pdf --new-password s3cret -o locked.pdf"],
    tags: &["password", "protect", "lock", "secure", "aes"],
    suffix: Some("encrypted"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let mut doc = open(job.input(), p)?;
    let password = p.text("new-password");
    if password.is_empty() {
        return Err(bad_value("new-password", "must not be empty"));
    }
    let owner = p.opt_text("owner-password");
    deft_tools_pdf::meta::encrypt(
        &mut doc,
        &password,
        owner.as_deref(),
        p.flag("no-print"),
        p.flag("no-copy"),
    )?;
    let bytes = deft_tools_pdf::meta::save_encrypted(&mut doc)?;
    Ok(OpResult::one(Produced::bytes(bytes, Format::Pdf)))
}
