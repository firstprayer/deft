use super::_common::{OUT, SHEET, TO, sheet_index, write};
use deft_core::prelude::*;
use deft_tools_sheet::Database;
use std::collections::HashSet;

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
        "Each file is a table named after it; a workbook's sheets are also tables by sheet name.",
        "--sheet picks the sheet the file name means. A SQLite file keeps its own tables.",
        "Numbers compare as numbers and keep every digit; ids such as 007 stay text. x regexp '^A' works.",
        "Empty cells are NULL. A repeated header gets _2. List tables: select name from sqlite_master.",
    ],
    input: Input::all(FORMATS, 1, "sheet|sqlite"),
    output: Output::one(OUT, "sheet"),
    params: &[
        param("sql", Kind::Text, "SELECT statement").required(),
        SHEET,
        TO,
    ],
    examples: &[
        "deft sheet query people.csv --sql 'select name, age from people where age > 30 order by age desc' -o adults.csv",
        "deft sheet query people.csv cities.csv --sql 'select * from people join cities using (name)' -o j.csv",
        "deft sheet query sales.csv --sql 'select city, sum(amount) t from sales group by city' -o t.csv",
        "deft sheet query sales.csv --sql 'select distinct city from sales' -o unique.csv",
    ],
    tags: &[
        "sql",
        "sqlite",
        "database",
        "select from csv",
        "db to csv",
        "export table",
        "filter rows",
        "sort rows",
        "join tables",
        "remove duplicates",
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

/// Take a table name, adding _2, _3 until it is free. SQLite ignores case.
fn claim(used: &mut HashSet<String>, base: String) -> String {
    let (mut name, mut n) = (base.clone(), 1);
    while !used.insert(name.to_lowercase()) {
        n += 1;
        name = format!("{base}_{n}");
    }
    name
}

/// Load one input: a table named after the file, and one per further sheet.
fn load(
    job: &Job,
    db: &mut Database,
    input: &InputFile,
    used: &mut HashSet<String>,
    warnings: &mut Vec<String>,
) -> Result<()> {
    let sheets = super::_common::tables(input)?;
    let workbook = matches!(input.format, Some(Format::Xlsx | Format::Xls | Format::Ods));
    let main = match job.params.opt_text("sheet") {
        Some(w) if workbook => sheet_index(&sheets, &w)?,
        _ => 0,
    };
    let base = table_name(&input.stem());
    let name = claim(used, base.clone());
    if name != base {
        warnings.push(format!(
            "two inputs are called {base}: {} is table {name}",
            input.display
        ));
    }
    let mut add = |db: &mut Database, sheet, name: &str| -> Result<()> {
        let mixed = db.add(sheet, name)?;
        if !mixed.is_empty() {
            warnings.push(format!(
                "{name}: {} kept as text to save spellings such as 007, so it compares as text; \
                 use cast(column as real) for numbers",
                mixed.join(", ")
            ));
        }
        Ok(())
    };
    add(db, &sheets[main], &name)?;
    if workbook {
        let alias = table_name(&sheets[main].name);
        if used.insert(alias.to_lowercase()) {
            db.alias(&alias, &name)?;
        }
        for (i, sheet) in sheets.iter().enumerate() {
            if i != main {
                let name = claim(used, table_name(&sheet.name));
                add(db, sheet, &name)?;
            }
        }
    }
    Ok(())
}

fn run(job: &Job) -> Result<OpResult> {
    let p = job.params;
    let sql = p.text("sql");
    if sql.trim_start().starts_with('.') {
        return Err(bad_value("sql", "give a SQL statement, not a dot command"));
    }
    let file = job.inputs.iter().find(|i| i.format == Some(Format::Sqlite));
    if file.is_some() && job.inputs.len() > 1 {
        return Err(err(
            ErrorCode::BadOption,
            "give one SQLite file, or any number of tables",
        ));
    }
    let mut warnings = Vec::new();
    let db = match file {
        Some(f) => Database::open_read_only(&f.local_path(job.ctx)?)?,
        None => {
            let mut db = Database::in_memory()?;
            let mut used = HashSet::new();
            for input in &job.inputs {
                load(job, &mut db, input, &mut used, &mut warnings)?;
            }
            db
        }
    };
    let table = db.query(&sql, job.ctx.timeout)?;
    let format = p
        .opt_text("to")
        .and_then(|t| Format::from_name(&t))
        .unwrap_or_else(|| job.target(Format::Csv));
    let rows = table.rows.len().saturating_sub(1);
    let mut res = OpResult::one(write(&table, format)?)
        .fact("rows", rows)
        .fact("columns", table.columns())
        .note(format!("{rows} rows"));
    for w in warnings {
        res = res.warn(w);
    }
    Ok(res)
}
