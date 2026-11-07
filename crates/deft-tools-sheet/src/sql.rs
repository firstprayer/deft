//! SQL over tables, on SQLite compiled into deft. Values go in and come out
//! through the API, so a number is never rewritten as text on the way.

use crate::{Cell, Table, numeric, parse_scalar};
use deft_core::error::{DeftError, ErrorCode, Result, err};
use regex::Regex;
use rusqlite::fallible_iterator::FallibleIterator;
use rusqlite::functions::FunctionFlags;
use rusqlite::limits::Limit;
use rusqlite::types::{Value, ValueRef};
use rusqlite::{Batch, Connection, OpenFlags, Statement, params_from_iter};
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// One column as SQLite should see it.
#[derive(Clone, Debug, PartialEq)]
pub struct SqlColumn {
    pub name: String,
    /// False keeps spellings such as 007 that the NUMERIC type would rewrite.
    pub numeric: bool,
    /// Holds real numbers as well as identifiers, so it compares as text.
    pub mixed: bool,
}

pub fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Text that the NUMERIC type would turn into a different number.
fn identifier(c: &Cell) -> bool {
    let Cell::Text(s) = c else { return false };
    let t = s.trim();
    t.starts_with(|c: char| c.is_ascii_digit() || "+-.".contains(c))
        && t.parse::<f64>().is_ok()
        && exact_integer(s).is_none()
        && matches!(parse_scalar(s, false), Cell::Text(_))
}

/// A whole number that SQLite holds exactly, though a double could not.
fn exact_integer(s: &str) -> Option<i64> {
    s.parse().ok().filter(|n: &i64| n.to_string() == s)
}

/// Names SQLite accepts: none empty, none repeated, ignoring case.
fn column_names(t: &Table) -> Vec<String> {
    crate::unique_names(&crate::headers(t), t.columns().max(1))
}

pub fn sql_columns(t: &Table) -> Vec<SqlColumn> {
    let body = t.rows.get(1..).unwrap_or_default();
    column_names(t)
        .into_iter()
        .enumerate()
        .map(|(i, name)| {
            let cells = || body.iter().filter_map(|r| r.get(i));
            let numeric = !cells().any(identifier);
            let mixed = !numeric && cells().any(|c| c.number().is_some());
            SqlColumn {
                name,
                numeric,
                mixed,
            }
        })
        .collect()
}

/// A cell as SQLite stores it. An empty cell is NULL, so it is no value.
fn sql_value(c: &Cell, column: &SqlColumn) -> Value {
    match c {
        Cell::Empty => Value::Null,
        Cell::Text(s) if s.is_empty() => Value::Null,
        other if !column.numeric => Value::Text(other.to_string()),
        Cell::Number(n) | Cell::Decimal(n, _) => Value::Real(*n),
        Cell::Text(s) => match (exact_integer(s), numeric(c)) {
            (Some(n), _) => Value::Integer(n),
            (None, Some(n)) => Value::Real(n),
            _ => Value::Text(s.clone()),
        },
        other => Value::Text(other.to_string()),
    }
}

/// `x regexp 'pattern'`. SQLite has the syntax but leaves the function to the
/// program, and it is how a query matches a pattern.
fn add_regexp(conn: &Connection) -> rusqlite::Result<()> {
    let flags = FunctionFlags::SQLITE_UTF8 | FunctionFlags::SQLITE_DETERMINISTIC;
    conn.create_scalar_function("regexp", 2, flags, |ctx| {
        let pattern: Arc<Regex> = ctx.get_or_create_aux(0, |v| {
            let compiled: std::result::Result<Regex, Box<dyn std::error::Error + Send + Sync>> = v
                .as_str()
                .map_err(Into::into)
                .and_then(|p| Regex::new(p).map_err(Into::into));
            compiled
        })?;
        Ok(match ctx.get_raw(1) {
            ValueRef::Null => None,
            ValueRef::Integer(i) => Some(pattern.is_match(&i.to_string())),
            ValueRef::Real(n) => Some(pattern.is_match(&n.to_string())),
            ValueRef::Text(b) | ValueRef::Blob(b) => {
                Some(pattern.is_match(&String::from_utf8_lossy(b)))
            }
        })
    })
}

fn failed(e: rusqlite::Error) -> DeftError {
    use rusqlite::ErrorCode::{DatabaseCorrupt, NotADatabase};
    let code = match e.sqlite_error_code() {
        Some(DatabaseCorrupt | NotADatabase) => ErrorCode::InputCorrupt,
        _ => ErrorCode::BadValue,
    };
    err(code, e.to_string())
}

/// How to open a file in WAL mode that has no -wal file beside it. Nothing is
/// pending, and opened the usual way SQLite would leave -shm and -wal files in
/// the user's folder, or fail where it may not write them.
fn immutable_uri(path: &Path) -> Option<String> {
    let mut head = [0u8; 20];
    std::fs::File::open(path)
        .and_then(|mut f| f.read_exact(&mut head))
        .ok()?;
    let wal = path.with_file_name(format!("{}-wal", path.file_name()?.to_string_lossy()));
    if head[18] != 2 || wal.exists() {
        return None;
    }
    let text = path.to_string_lossy().replace('\\', "/");
    let encoded: String = text
        .bytes()
        .map(|b| match b {
            b'/' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            b if b.is_ascii_alphanumeric() => (b as char).to_string(),
            b => format!("%{b:02X}"),
        })
        .collect();
    Some(format!("file:{encoded}?immutable=1"))
}

/// A database to query: tables loaded into memory, or a file opened read-only.
pub struct Database {
    conn: Connection,
    /// How each source number was written, by its exact value.
    spellings: HashMap<u64, String>,
}

impl Database {
    fn new(conn: Connection) -> Result<Database> {
        // ATTACH could open, and so write, any other file.
        conn.set_limit(Limit::SQLITE_LIMIT_ATTACHED, 0)
            .map_err(failed)?;
        add_regexp(&conn).map_err(failed)?;
        Ok(Database {
            conn,
            spellings: HashMap::new(),
        })
    }

    pub fn in_memory() -> Result<Database> {
        Database::new(Connection::open_in_memory().map_err(failed)?)
    }

    pub fn open_read_only(path: &Path) -> Result<Database> {
        let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_NO_MUTEX
            | OpenFlags::SQLITE_OPEN_URI;
        let conn = match immutable_uri(path) {
            Some(uri) => Connection::open_with_flags(uri, flags),
            None => Connection::open_with_flags(path, flags),
        };
        let conn =
            conn.map_err(|e| err(ErrorCode::InputCorrupt, format!("{}: {e}", path.display())))?;
        Database::new(conn)
    }

    /// Load a table in one transaction. Returns the columns that hold numbers
    /// but must compare as text.
    pub fn add(&mut self, t: &Table, name: &str) -> Result<Vec<String>> {
        let columns = sql_columns(t);
        let declared: Vec<String> = columns
            .iter()
            .map(|c| {
                let kind = if c.numeric { "NUMERIC" } else { "TEXT" };
                format!("{} {kind}", quoted(&c.name))
            })
            .collect();
        let tx = self.conn.transaction().map_err(failed)?;
        tx.execute_batch(&format!(
            "create table {}({});",
            quoted(name),
            declared.join(", ")
        ))
        .map_err(failed)?;
        let marks = vec!["?"; columns.len()].join(", ");
        let insert = format!("insert into {} values ({marks})", quoted(name));
        {
            let mut insert = tx.prepare(&insert).map_err(failed)?;
            for row in t.rows.iter().skip(1) {
                let values = columns.iter().enumerate().map(|(i, column)| {
                    row.get(i)
                        .map_or(Value::Null, |cell| sql_value(cell, column))
                });
                insert.execute(params_from_iter(values)).map_err(failed)?;
            }
        }
        tx.commit().map_err(failed)?;
        for c in t.rows.iter().skip(1).flatten() {
            if let Cell::Number(n) | Cell::Decimal(n, _) = c {
                self.remember(*n, c.to_string());
            }
        }
        Ok(columns
            .into_iter()
            .filter(|c| c.mixed)
            .map(|c| c.name)
            .collect())
    }

    /// 3,50 is read as a number but is no way to write one in a result, and
    /// two spellings of one value leave only the plain one.
    fn remember(&mut self, n: f64, spelling: String) {
        let plain = || Cell::Number(n).to_string();
        let spelling = if spelling.contains(',') {
            plain()
        } else {
            spelling
        };
        self.spellings
            .entry(n.to_bits())
            .and_modify(|known| {
                if *known != spelling {
                    *known = plain();
                }
            })
            .or_insert(spelling);
    }

    /// Give a table a second name.
    pub fn alias(&self, alias: &str, name: &str) -> Result<()> {
        self.conn
            .execute_batch(&format!(
                "create view {} as select * from {};",
                quoted(alias),
                quoted(name)
            ))
            .map_err(failed)
    }

    /// Each table with its columns, to help with a name that was not found.
    fn describe(&self) -> Option<String> {
        let mut stmt = self
            .conn
            .prepare(
                "select m.name, group_concat(c.name, ', ') from sqlite_master m, \
                 pragma_table_info(m.name) c where m.type in ('table', 'view') group by m.name",
            )
            .ok()?;
        let rows = stmt.query_map([], |r| {
            Ok(format!(
                "{} ({})",
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?
            ))
        });
        let tables: Vec<String> = rows.ok()?.filter_map(|r| r.ok()).collect();
        (!tables.is_empty()).then(|| format!("tables: {}", tables.join("; ")))
    }

    fn cell(&self, value: ValueRef, direct: bool) -> Cell {
        match value {
            ValueRef::Null => Cell::Empty,
            ValueRef::Integer(i) => {
                // NUMERIC stores 2500.0 as an integer. A count is no source cell.
                let n = i as f64;
                match self.spellings.get(&n.to_bits()) {
                    Some(s) if direct && n as i64 == i => Cell::Decimal(n, s.clone()),
                    _ => parse_scalar(&i.to_string(), false),
                }
            }
            ValueRef::Real(n) if !n.is_finite() => Cell::Text(n.to_string()),
            ValueRef::Real(n) => match self.spellings.get(&n.to_bits()) {
                Some(s) => Cell::Decimal(n, s.clone()),
                None if direct => Cell::Number(n),
                // Binary sums drift, as 0.1 + 0.2 does: keep 15 digits.
                None => Cell::Number(format!("{n:.14e}").parse().unwrap_or(n)),
            },
            ValueRef::Text(b) => Cell::Text(String::from_utf8_lossy(b).into_owned()),
            ValueRef::Blob(b) => match std::str::from_utf8(b) {
                Ok(s) => Cell::Text(s.to_string()),
                Err(_) => Cell::Text(b.iter().map(|x| format!("{x:02X}")).collect()),
            },
        }
    }

    fn rows(&self, stmt: &mut Statement) -> Result<Table> {
        let direct: Vec<bool> = stmt
            .columns_with_metadata()
            .iter()
            .map(|c| c.origin_name().is_some())
            .collect();
        let head = stmt.column_names();
        let head: Vec<Cell> = head.iter().map(|n| Cell::Text(n.to_string())).collect();
        let mut table = Table {
            name: "query".into(),
            rows: vec![head],
            ..Table::default()
        };
        let mut rows = stmt.query([]).map_err(interrupted)?;
        while let Some(row) = rows.next().map_err(interrupted)? {
            let mut cells = Vec::with_capacity(direct.len());
            for (i, direct) in direct.iter().enumerate() {
                cells.push(self.cell(row.get_ref(i).map_err(failed)?, *direct));
            }
            table.rows.push(cells);
        }
        Ok(table)
    }

    /// Run SQL that only reads. With several statements, the last that
    /// returns rows is the result.
    pub fn query(&self, sql: &str, timeout: Option<Duration>) -> Result<Table> {
        // A second lock behind the check below, for the in-memory tables too.
        self.conn
            .execute_batch("pragma query_only = on;")
            .map_err(failed)?;
        if let Some(limit) = timeout {
            let started = Instant::now();
            self.conn
                .progress_handler(100_000, Some(move || started.elapsed() > limit))
                .map_err(failed)?;
        }
        let with_hint = |e: rusqlite::Error| {
            let mut e = failed(e);
            if e.message.contains("no such table") || e.message.contains("no such column") {
                e.hint = self.describe();
            }
            e
        };
        let mut batch = Batch::new(&self.conn, sql);
        let mut result = None;
        while let Some(mut stmt) = batch.next().map_err(with_hint)? {
            if !stmt.readonly() {
                return Err(err(ErrorCode::BadValue, "--sql can only read")
                    .hint("give a SELECT; deft never changes a database"));
            }
            if stmt.column_count() > 0 {
                result = Some(self.rows(&mut stmt)?);
            }
        }
        result.ok_or_else(|| err(ErrorCode::BadValue, "--sql has no SELECT"))
    }
}

fn interrupted(e: rusqlite::Error) -> DeftError {
    if e.sqlite_error_code() == Some(rusqlite::ErrorCode::OperationInterrupted) {
        return err(ErrorCode::Timeout, "the query exceeded --timeout");
    }
    failed(e)
}

#[cfg(test)]
#[path = "sql_tests.rs"]
mod tests;
