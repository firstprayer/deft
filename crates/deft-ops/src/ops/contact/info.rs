use super::_common::load;
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "contact.info",
    summary: "Count contacts, phones and email addresses",
    input: Input::each(&[Format::Vcf], "vcf"),
    output: Output::report(),
    examples: &["deft contact info contacts.vcf"],
    tags: &["address book", "how many contacts"],
    result: &["contacts", "phones", "emails"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let contacts = load(job.input())?;
    let phones: usize = contacts.iter().map(|c| c.phones.len()).sum();
    let emails: usize = contacts.iter().map(|c| c.emails.len()).sum();
    let note = format!(
        "{} contact{}",
        contacts.len(),
        if contacts.len() == 1 { "" } else { "s" }
    );
    Ok(OpResult::report(
        facts![
            ("contacts", contacts.len()),
            ("phones", phones),
            ("emails", emails)
        ],
        note,
    ))
}
