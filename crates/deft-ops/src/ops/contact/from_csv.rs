use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "contact.from-csv",
    summary: "Make a VCF address book from a table",
    details: &["Needs a name column. Separate several phones or emails with ;."],
    input: Input::each(&[Format::Csv], "csv"),
    output: Output::one(&[Format::Vcf], "vcf"),
    examples: &["deft contact from-csv addresses.csv -o contacts.vcf"],
    tags: &["csv to vcf", "import contacts", "vcard"],
    result: &["contacts"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    let contacts = deft_tools_data::vcard::from_csv(job.input().bytes()?)
        .map_err(|e| err(ErrorCode::InputCorrupt, e))?;
    let text = deft_tools_data::vcard::write(&contacts);
    Ok(
        OpResult::one(Produced::bytes(text.into_bytes(), Format::Vcf))
            .fact("contacts", contacts.len())
            .note(format!(
                "{} contact{}",
                contacts.len(),
                if contacts.len() == 1 { "" } else { "s" }
            )),
    )
}
