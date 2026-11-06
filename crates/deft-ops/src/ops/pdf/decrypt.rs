use super::_common::{PASSWORD, open, write};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "pdf.decrypt",
    summary: "Remove a password you know",
    details: &["Files with an empty user password need no --password."],
    input: Input::each(&[Format::Pdf], "pdf"),
    output: Output::one(&[Format::Pdf], "pdf"),
    params: &[PASSWORD],
    examples: &["deft pdf decrypt report.pdf -o open.pdf"],
    tags: &["unlock", "remove password", "unprotect"],
    errors: &[ErrorCode::InputEncrypted],
    suffix: Some("decrypted"),
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let mut doc = open(job.input(), job.params)?;
    Ok(OpResult::one(write(&mut doc)?))
}
