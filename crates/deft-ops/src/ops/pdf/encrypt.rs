use super::_common::{PASSWORD, open};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "pdf.encrypt",
    summary: "Protect with a password (AES-256)",
    details: &[
        "--new-password opens the file. --password is for an already locked input.",
        "Use an ASCII password: Preview and other Apple viewers cannot open a PDF whose password has other characters.",
        "--no-print and --no-copy are requests: most viewers honour them, but a tool that ignores them can still print and copy.",
    ],
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::one(&[Format::Pdf], "pdf"),
    params: &[
        param("new-password", Kind::Text, "Password to open the output").required(),
        param(
            "owner-password",
            Kind::Text,
            "Password for full control (default: the same)",
        ),
        param("no-print", Kind::Flag, "Ask viewers not to allow printing"),
        param(
            "no-copy",
            Kind::Flag,
            "Ask viewers not to allow copying text",
        ),
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
    let bytes = deft_tools_pdf::meta::save_protected(&mut doc)?;
    let mut res = OpResult::one(Produced::bytes(bytes, Format::Pdf));
    if let Some(w) = non_ascii_warning(&password, owner.as_deref()) {
        res = res.warn(w);
    }
    if p.flag("no-print") || p.flag("no-copy") {
        res = res.warn(
            "--no-print and --no-copy are requests: most viewers honour them, \
             but a tool that ignores them can still print and copy",
        );
    }
    Ok(res)
}

/// AES-256 passwords may have any characters (UTF-8), but macOS and iOS
/// (Preview, Quick Look, Safari) only accept ASCII ones, so a Mac user could
/// not open their own file.
fn non_ascii_warning(password: &str, owner: Option<&str>) -> Option<String> {
    let odd: String = password
        .chars()
        .chain(owner.unwrap_or("").chars())
        .filter(|c| !c.is_ascii())
        .collect();
    let first = odd.chars().next()?;
    Some(format!(
        "the password has characters outside ASCII ({first}): Preview and other Apple viewers \
         cannot open the file with it, although viewers that follow the PDF standard can. \
         Use an ASCII password if the file will be opened on a Mac, iPhone or iPad"
    ))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_password_apple_viewers_cannot_use_is_named() {
        assert!(super::non_ascii_warning("Statement-2026", None).is_none());
        let w = super::non_ascii_warning("Müller-2026", None).unwrap();
        assert!(w.contains("(ü)") && w.contains("Preview"), "{w}");
        assert!(super::non_ascii_warning("plain", Some("密码")).is_some());
    }
}
