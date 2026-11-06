use super::_common::{PASSWORD, open};
use deft_core::prelude::*;

pub static OP: Op = Op {
    id: "pdf.compare",
    summary: "Compare two PDFs: text and pixels",
    details: &["--text-only skips rendering. Reports lines added and removed per page."],
    input: Input::all(&[Format::Pdf], 2, "pdf"),
    output: Output::report(),
    params: &[
        param("text-only", Kind::Flag, "Do not compare the pictures"),
        param(
            "dpi",
            Kind::Int { min: 36, max: 300 },
            "Resolution of the picture check",
        )
        .default("72"),
        PASSWORD,
    ],
    examples: &["deft pdf compare report.pdf appendix.pdf --json"],
    tags: &["diff", "what changed", "versions", "redline"],
    result: &["identical", "pages", "changes"],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    if job.inputs.len() != 2 {
        return Err(err(ErrorCode::BadOption, "give exactly two PDFs"));
    }
    let p = job.params;
    let mut a = open(&job.inputs[0], p)?;
    let mut b = open(&job.inputs[1], p)?;
    let visual = !p.flag("text-only");
    let (ba, bb) = if visual {
        (deft_tools_pdf::save(&mut a)?, deft_tools_pdf::save(&mut b)?)
    } else {
        (vec![], vec![])
    };
    let renderers = if visual {
        Some((
            deft_tools_pdf::Renderer::new(&ba)?,
            deft_tools_pdf::Renderer::new(&bb)?,
        ))
    } else {
        None
    };
    let changes = deft_tools_pdf::compare::compare(
        &a,
        &b,
        renderers.as_ref().map(|r| &r.0),
        renderers.as_ref().map(|r| &r.1),
        p.int("dpi") as f64,
        job.ctx.limits.max_pixels,
    )?;
    let (na, nb) = (
        deft_tools_pdf::page_ids(&a).len(),
        deft_tools_pdf::page_ids(&b).len(),
    );
    let list: Vec<_> = changes.iter().map(|c| c.to_json()).collect();
    let identical = list.is_empty() && na == nb;
    let note = if identical {
        "no differences".to_string()
    } else {
        format!("{} of {} pages differ", list.len(), na.max(nb))
    };
    Ok(OpResult::report(
        facts![
            ("identical", identical),
            ("pages", [na, nb]),
            ("changes", list)
        ],
        note,
    ))
}
