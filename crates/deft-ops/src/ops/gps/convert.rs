use super::_common::{IN, load};
use deft_core::prelude::*;
use deft_tools_data::geo;

const OUT: &[Format] = &[Format::Gpx, Format::Kml, Format::Csv];

pub static OP: Op = Op {
    id: "gps.convert",
    summary: "Change format: --to gpx|kml|csv",
    details: &["KML has no times, so a GPX track loses them when converted to KML."],
    input: Input::each(IN, "gpx|kml"),
    output: Output::one(OUT, "gpx|kml|csv"),
    params: &[param(
        "to",
        Kind::Choice(&["gpx", "kml", "csv"]),
        "Target format (or give -o name.ext)",
    )],
    converts: &[Edge {
        from: IN,
        to: OUT,
        quality: Quality::Approximate,
    }],
    examples: &[
        "deft gps convert ride.gpx -o ride.kml",
        "deft gps convert places.kml --to csv -o places.csv",
    ],
    tags: &[
        "gpx to kml",
        "kml to gpx",
        "google earth",
        "route",
        "waypoints",
    ],
    run,
    ..Op::DEFAULT
};

fn run(job: &Job) -> Result<OpResult> {
    if !job.params.has("to") && job.out_format.is_none() {
        return Err(err(
            ErrorCode::BadOption,
            "--to is required (or give -o name.ext)",
        ));
    }
    let data = load(job.input())?;
    let format = job
        .params
        .opt_text("to")
        .and_then(|t| Format::from_name(&t))
        .unwrap_or_else(|| job.target(Format::Gpx));
    let bytes = match format {
        Format::Kml => geo::write_kml(&data).into_bytes(),
        Format::Csv => geo::to_csv(&data).map_err(|e| err(ErrorCode::OutputUnwritable, e))?,
        _ => geo::write_gpx(&data).into_bytes(),
    };
    Ok(OpResult::one(Produced::bytes(bytes, format)))
}
