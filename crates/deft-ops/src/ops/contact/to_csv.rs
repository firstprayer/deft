use super::_common::load;
use deft_core::prelude::*;

const IN: &[Format] = &[Format::Vcf];

pub static OP: Op = Op {
    id: "contact.to-csv",
    summary: "Turn a VCF address book into a table",
    input: Input::each(IN, "vcf"),
    output: Output::one(&[Format::Csv], "csv"),
    converts: &[Edge {
        from: IN,
        to: &[Format::Csv],
        quality: Quality::Faithful,
    }],
    examples: &["deft contact to-csv contacts.vcf -o contacts.csv"],
    tags: &["export contacts", "address book", "vcard"],
    result: &["contacts"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let contacts = load(job.input())?;
    let bytes = deft_tools_data::vcard::to_csv(&contacts)
        .map_err(|e| err(ErrorCode::OutputUnwritable, e))?;
    Ok(OpResult::one(Produced::bytes(bytes, Format::Csv))
        .fact("contacts", contacts.len())
        .note(format!(
            "{} contact{}",
            contacts.len(),
            if contacts.len() == 1 { "" } else { "s" }
        )))
}
