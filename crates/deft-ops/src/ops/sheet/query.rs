use super::_common::{OUT, TO, write};
use deft_core::prelude::*;
use deft_tools_sheet::Table;

const FORMATS: &[Format] = &[
    Format::Csv,
    Format::Xlsx,
    Format::Xls,
    Format::Ods,
    Format::Json,
    Format::Sqlite,
];

pub static OP: Op = Op {
    id: "sheet.query",
    summary: "Run SQL over tables or a SQLite file: --sql 'select ...'",
    details: &[
        "Each file becomes a table named after it. A SQLite file keeps its own tables.",
        "Columns with numbers compare as numbers. List tables: select name from sqlite_master.",
    ],
    input: Input::all(FORMATS, 1, "sheet|sqlite"),
    output: Output::one(OUT, "sheet"),
    params: &[param("sql", Kind::Text, "SELECT statement").required(), TO],
    engines: &["sqlite3"],
    examples: &[
        "deft sheet query sales.csv --sql 'select city, sum(amount) t from sales group by city' -o t.csv",
        "deft sheet query people.csv cities.csv --sql 'select * from people join cities using (name)' -o j.csv",
    ],
    tags: &[
        "sql",
        "sqlite",
        "database",
        "select from csv",
        "db to csv",
        "export table",
    ],
    result: &["rows", "columns"],
    suffix: Some("query"),
    run,
    ..Op::DEFAULT
};

/// A safe SQL identifier for a file name.
fn table_name(stem: &str) -> String {
    let mut s: String = stem
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    if s.starts_with(|c: char| c.is_ascii_digit()) {
        s.insert(0, 't');
    }
    s
}

fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// `create table` and `.import` commands for one table.
fn load_commands(job: &Job, t: &Table, name: &str) -> Result<Vec<String>> {
    let csv = job.ctx.temp_path(&format!("{name}.csv"));
    std::fs::write(&csv, deft_tools_sheet::write_csv(t)?)?;
    let columns: Vec<String> = deft_tools_sheet::headers(t)
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let name = if h.is_empty() {
                format!("column{}", i + 1)
            } else {
                h.clone()
            };
            format!("{} NUMERIC", quoted(&name))
        })
        .collect();
    Ok(vec![
        format!("create table {}({});", quoted(name), columns.join(", ")),
        format!(".import --csv --skip 1 '{}' {name}", csv.display()),
    ])
}

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    // The sqlite3 shell treats these as its own commands, such as .shell.
    if p.text("sql").trim_start().starts_with('.') {
        return Err(bad_value("sql", "give a SQL statement, not a dot command"));
    }
    let program = deft_engines::require(job.ctx, "sqlite3", OP.id)?;
    let mut args = vec![
        "-batch".to_string(),
        "-csv".to_string(),
        "-header".to_string(),
    ];
    let database = job.inputs.iter().find(|i| i.format == Some(Format::Sqlite));
    match database {
        Some(db) => {
            args.push("-readonly".to_string());
            args.push(db.local_path(job.ctx)?.display().to_string());
        }
        None => args.push(":memory:".to_string()),
    }
    if database.is_some() && job.inputs.len() > 1 {
        return Err(err(
            ErrorCode::BadOption,
            "give one SQLite file, or any number of tables",
        ));
    }
    if database.is_none() {
        for input in &job.inputs {
            let mut tables = super::_common::tables(input)?;
            let t = tables.remove(0);
            for command in load_commands(job, &t, &table_name(&input.stem()))? {
                args.extend(["-cmd".to_string(), command]);
            }
        }
    }
    args.push(p.text("sql"));
    let out = deft_engines::run(job.ctx, &program, &args).map_err(|mut e| {
        e.code = ErrorCode::BadValue;
        e
    })?;
    let table = deft_tools_sheet::read_csv(&out.stdout, "query")?;
    let format = p
        .opt_text("to")
        .and_then(|t| Format::from_name(&t))
        .unwrap_or_else(|| job.target(Format::Csv));
    Ok(OpResult::one(write(&table, format)?)
        .fact("rows", table.rows.len().saturating_sub(1))
        .fact("columns", table.columns())
        .note(format!("{} rows", table.rows.len().saturating_sub(1))))
}
