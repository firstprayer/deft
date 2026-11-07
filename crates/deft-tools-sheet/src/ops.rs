//! Table transforms. The first row of a table is its header.

use crate::{Cell, Filter, Table};
use regex::Regex;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::LazyLock;

pub fn headers(t: &Table) -> Vec<String> {
    t.rows
        .first()
        .map(|r| r.iter().map(|c| c.to_string()).collect())
        .unwrap_or_default()
}

/// Find a column by exact name, then ignoring case, then by 1-based number.
pub fn find_column(t: &Table, name: &str) -> Result<usize, String> {
    let head = headers(t);
    let name = name.trim();
    if let Some(i) = head.iter().position(|h| h == name) {
        return Ok(i);
    }
    if let Some(i) = head.iter().position(|h| h.eq_ignore_ascii_case(name)) {
        return Ok(i);
    }
    if let Ok(n) = name.parse::<usize>()
        && n >= 1
        && n <= head.len()
    {
        return Ok(n - 1);
    }
    Err(format!("no column {name:?}; columns: {}", head.join(", ")))
}

/// Columns from a comma-separated list.
pub fn find_columns(t: &Table, list: &str) -> Result<Vec<usize>, String> {
    list.split(',').map(|c| find_column(t, c)).collect()
}

fn with_rows(t: &Table, rows: Vec<Vec<Cell>>) -> Table {
    Table {
        name: t.name.clone(),
        rows,
        ..Table::default()
    }
}

fn body(t: &Table) -> &[Vec<Cell>] {
    t.rows.get(1..).unwrap_or_default()
}

fn head_row(t: &Table) -> Vec<Vec<Cell>> {
    t.rows.first().cloned().into_iter().collect()
}

pub fn filter(t: &Table, f: &Filter) -> Table {
    let mut rows = head_row(t);
    rows.extend(body(t).iter().filter(|r| f.matches(r)).cloned());
    with_rows(t, rows)
}

/// Numbers before text; empty cells always last.
pub fn compare_cells(a: &Cell, b: &Cell) -> Ordering {
    let empty = |c: &Cell| c.to_string().is_empty();
    match (empty(a), empty(b)) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Greater,
        (false, true) => return Ordering::Less,
        _ => {}
    }
    match (a.number(), b.number()) {
        (Some(x), Some(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        _ => a.to_string().cmp(&b.to_string()),
    }
}

/// Stable sort by columns, in order of importance.
pub fn sort(t: &Table, columns: &[usize], descending: bool) -> Table {
    let mut rows: Vec<Vec<Cell>> = body(t).to_vec();
    let empty = Cell::Empty;
    rows.sort_by(|a, b| {
        for &c in columns {
            let (x, y) = (a.get(c).unwrap_or(&empty), b.get(c).unwrap_or(&empty));
            let blank = x.to_string().is_empty() || y.to_string().is_empty();
            let mut o = compare_cells(x, y);
            if descending && !blank {
                o = o.reverse();
            }
            if o != Ordering::Equal {
                return o;
            }
        }
        Ordering::Equal
    });
    let mut out = head_row(t);
    out.extend(rows);
    with_rows(t, out)
}

fn key(row: &[Cell], columns: &[usize]) -> Vec<String> {
    columns
        .iter()
        .map(|&c| row.get(c).map(|x| x.to_string()).unwrap_or_default())
        .collect()
}

/// Drop repeated rows, comparing `columns` or the whole row. Keeps the first
/// occurrence, or the last with `keep_last`, in the original order.
pub fn dedupe(t: &Table, columns: Option<&[usize]>, keep_last: bool) -> Table {
    let all: Vec<usize> = (0..t.columns()).collect();
    let columns = columns.unwrap_or(&all);
    let rows = body(t);
    let mut seen = HashSet::new();
    let mut keep = vec![false; rows.len()];
    let order: Box<dyn Iterator<Item = usize>> = if keep_last {
        Box::new((0..rows.len()).rev())
    } else {
        Box::new(0..rows.len())
    };
    for i in order {
        keep[i] = seen.insert(key(&rows[i], columns));
    }
    let mut out = head_row(t);
    out.extend(
        rows.iter()
            .zip(&keep)
            .filter(|(_, k)| **k)
            .map(|(r, _)| r.clone()),
    );
    with_rows(t, out)
}

pub fn select(t: &Table, columns: &[usize]) -> Table {
    let rows = t
        .rows
        .iter()
        .map(|r| {
            columns
                .iter()
                .map(|&c| r.get(c).cloned().unwrap_or(Cell::Empty))
                .collect()
        })
        .collect();
    with_rows(t, rows)
}

pub fn drop_columns(t: &Table, columns: &[usize]) -> Table {
    let keep: Vec<usize> = (0..t.columns()).filter(|c| !columns.contains(c)).collect();
    select(t, &keep)
}

pub fn transpose(t: &Table) -> Table {
    let rows = (0..t.columns())
        .map(|c| {
            t.rows
                .iter()
                .map(|r| r.get(c).cloned().unwrap_or(Cell::Empty))
                .collect()
        })
        .collect();
    with_rows(t, rows)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Join {
    Inner,
    Left,
    Right,
    Outer,
}

/// Join on one key column from each side. The right key column is dropped.
pub fn join(l: &Table, r: &Table, lk: usize, rk: usize, how: Join) -> Table {
    let width = |t: &Table| t.columns();
    let (lw, rw) = (width(l), width(r));
    let cell_text = |row: &[Cell], c: usize| row.get(c).map(|x| x.to_string()).unwrap_or_default();
    let mut index: HashMap<String, Vec<usize>> = HashMap::new();
    let rbody = body(r);
    for (i, row) in rbody.iter().enumerate() {
        index.entry(cell_text(row, rk)).or_default().push(i);
    }
    let pad = |row: &[Cell], n: usize| {
        let mut v = row.to_vec();
        v.resize(n, Cell::Empty);
        v
    };
    let right_part = |row: &[Cell]| -> Vec<Cell> {
        pad(row, rw)
            .into_iter()
            .enumerate()
            .filter(|(i, _)| *i != rk)
            .map(|(_, c)| c)
            .collect()
    };
    let mut rows = Vec::new();
    if let (Some(a), Some(b)) = (l.rows.first(), r.rows.first()) {
        let left_names: Vec<String> = a.iter().map(|c| c.to_string()).collect();
        let mut head = pad(a, lw);
        for c in right_part(b) {
            let name = c.to_string();
            head.push(if left_names.contains(&name) {
                Cell::Text(format!("{name}_2"))
            } else {
                c
            });
        }
        rows.push(head);
    }
    let mut used = vec![false; rbody.len()];
    for row in body(l) {
        match index.get(&cell_text(row, lk)) {
            Some(hits) => {
                for &i in hits {
                    used[i] = true;
                    let mut out = pad(row, lw);
                    out.extend(right_part(&rbody[i]));
                    rows.push(out);
                }
            }
            None if matches!(how, Join::Left | Join::Outer) => {
                let mut out = pad(row, lw);
                out.resize(lw + rw.saturating_sub(1), Cell::Empty);
                rows.push(out);
            }
            None => {}
        }
    }
    if matches!(how, Join::Right | Join::Outer) {
        for (i, row) in rbody.iter().enumerate() {
            if !used[i] {
                let mut out = vec![Cell::Empty; lw];
                out[lk] = row.get(rk).cloned().unwrap_or(Cell::Empty);
                out.extend(right_part(row));
                rows.push(out);
            }
        }
    }
    with_rows(l, rows)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Agg {
    Sum,
    Mean,
    Count,
    Min,
    Max,
}

impl Agg {
    pub fn parse(s: &str) -> Agg {
        match s {
            "mean" => Agg::Mean,
            "count" => Agg::Count,
            "min" => Agg::Min,
            "max" => Agg::Max,
            _ => Agg::Sum,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Agg::Sum => "sum",
            Agg::Mean => "mean",
            Agg::Count => "count",
            Agg::Min => "min",
            Agg::Max => "max",
        }
    }
}

/// The number in a cell, reading 1,200 as 1200. A comma anywhere else is not
/// a thousands separator: 3,50 is not 350.
pub(crate) fn numeric(c: &Cell) -> Option<f64> {
    static GROUPED: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"^[-+]?[1-9][0-9]{0,2}(,[0-9]{3})+(\.[0-9]+)?$").expect("valid")
    });
    c.number().or_else(|| match c {
        Cell::Text(s) => {
            let s = s.trim();
            if s.contains(',') && !GROUPED.is_match(s) {
                return None;
            }
            s.replace(',', "")
                .parse()
                .ok()
                .filter(|n: &f64| n.is_finite())
        }
        _ => None,
    })
}

fn aggregate(cells: &[&Cell], agg: Agg) -> Cell {
    let filled: Vec<&&Cell> = cells.iter().filter(|c| !c.to_string().is_empty()).collect();
    if agg == Agg::Count {
        return Cell::Number(filled.len() as f64);
    }
    let nums: Vec<f64> = filled.iter().filter_map(|c| numeric(c)).collect();
    if nums.is_empty() {
        return Cell::Empty;
    }
    let sum: f64 = nums.iter().sum();
    // Binary sums of decimals drift; keep what a person would write down.
    let tidy = |n: f64| (n * 1e10).round() / 1e10;
    Cell::Number(tidy(match agg {
        Agg::Sum => sum,
        Agg::Mean => sum / nums.len() as f64,
        Agg::Min => nums.iter().copied().fold(f64::INFINITY, f64::min),
        Agg::Max => nums.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        Agg::Count => unreachable!(),
    }))
}

/// Group by `by` and aggregate each of `values`. With no value columns, counts rows.
pub fn group(t: &Table, by: &[usize], values: &[usize], agg: Agg) -> Table {
    let names = headers(t);
    let mut order: Vec<Vec<String>> = Vec::new();
    let mut groups: HashMap<Vec<String>, Vec<&Vec<Cell>>> = HashMap::new();
    for row in body(t) {
        let k = key(row, by);
        let entry = groups.entry(k.clone()).or_default();
        if entry.is_empty() {
            order.push(k);
        }
        entry.push(row);
    }
    let mut head: Vec<Cell> = by
        .iter()
        .map(|&c| Cell::Text(names.get(c).cloned().unwrap_or_default()))
        .collect();
    if values.is_empty() {
        head.push(Cell::Text("count".into()));
    }
    for &v in values {
        let name = names.get(v).cloned().unwrap_or_default();
        head.push(Cell::Text(format!("{name}_{}", agg.name())));
    }
    let mut rows = vec![head];
    let empty = Cell::Empty;
    for k in order {
        let members = &groups[&k];
        let first = members[0];
        let mut row: Vec<Cell> = by
            .iter()
            .map(|&c| first.get(c).cloned().unwrap_or(Cell::Empty))
            .collect();
        if values.is_empty() {
            row.push(Cell::Number(members.len() as f64));
        }
        for &v in values {
            let cells: Vec<&Cell> = members.iter().map(|r| r.get(v).unwrap_or(&empty)).collect();
            row.push(aggregate(&cells, agg));
        }
        rows.push(row);
    }
    with_rows(t, rows)
}

/// Spread `column` values across the top, one row per `row` value.
pub fn pivot(t: &Table, row: usize, column: usize, value: Option<usize>, agg: Agg) -> Table {
    let names = headers(t);
    let mut cols: Vec<String> = Vec::new();
    let mut order: Vec<String> = Vec::new();
    let mut labels: HashMap<String, Cell> = HashMap::new();
    let mut cells: BTreeMap<(String, String), Vec<&Cell>> = BTreeMap::new();
    let empty = Cell::Empty;
    for r in body(t) {
        let rk = r.get(row).unwrap_or(&empty);
        let (rt, ct) = (
            rk.to_string(),
            r.get(column).map(|c| c.to_string()).unwrap_or_default(),
        );
        if !labels.contains_key(&rt) {
            labels.insert(rt.clone(), rk.clone());
            order.push(rt.clone());
        }
        if !cols.contains(&ct) {
            cols.push(ct.clone());
        }
        let v = value.map_or(rk, |v| r.get(v).unwrap_or(&empty));
        cells.entry((rt, ct)).or_default().push(v);
    }
    cols.sort_by(|a, b| match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(Ordering::Equal),
        _ => a.cmp(b),
    });
    let mut head = vec![Cell::Text(names.get(row).cloned().unwrap_or_default())];
    head.extend(cols.iter().map(|c| Cell::Text(c.clone())));
    let mut rows = vec![head];
    let agg = if value.is_none() { Agg::Count } else { agg };
    for rk in &order {
        let mut out = vec![labels[rk].clone()];
        for c in &cols {
            out.push(match cells.get(&(rk.clone(), c.clone())) {
                // With no value column, every row counts, even one with a blank key.
                Some(v) if value.is_none() => Cell::Number(v.len() as f64),
                Some(v) => aggregate(v, agg),
                None => Cell::Empty,
            });
        }
        rows.push(out);
    }
    with_rows(t, rows)
}

/// Facts about one column.
#[derive(Clone, Debug, PartialEq)]
pub struct ColumnStats {
    pub name: String,
    pub filled: usize,
    pub empty: usize,
    pub unique: usize,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub mean: Option<f64>,
    pub sum: Option<f64>,
}

pub fn stats(t: &Table) -> Vec<ColumnStats> {
    let names = headers(t);
    (0..t.columns())
        .map(|c| {
            let cells: Vec<String> = body(t)
                .iter()
                .map(|r| r.get(c).map(|x| x.to_string()).unwrap_or_default())
                .collect();
            let filled: Vec<&String> = cells.iter().filter(|s| !s.is_empty()).collect();
            let nums: Vec<f64> = body(t)
                .iter()
                .filter_map(|r| r.get(c).and_then(numeric))
                .collect();
            // A column is numeric only when most of its filled cells are numbers.
            let is_numeric = !nums.is_empty() && nums.len() * 2 >= filled.len();
            let sum: f64 = nums.iter().sum();
            ColumnStats {
                name: names.get(c).cloned().unwrap_or_default(),
                filled: filled.len(),
                empty: cells.len() - filled.len(),
                unique: filled.iter().collect::<HashSet<_>>().len(),
                min: is_numeric.then(|| nums.iter().copied().fold(f64::INFINITY, f64::min)),
                max: is_numeric.then(|| nums.iter().copied().fold(f64::NEG_INFINITY, f64::max)),
                mean: is_numeric.then(|| sum / nums.len() as f64),
                sum: is_numeric.then_some(sum),
            }
        })
        .collect()
}

/// Split into one table per value of a column, in order of first appearance.
pub fn split_by(t: &Table, column: usize) -> Vec<(String, Table)> {
    let mut order: Vec<String> = Vec::new();
    let mut parts: HashMap<String, Vec<Vec<Cell>>> = HashMap::new();
    for r in body(t) {
        let k = r.get(column).map(|c| c.to_string()).unwrap_or_default();
        if !parts.contains_key(&k) {
            order.push(k.clone());
        }
        parts.entry(k).or_default().push(r.clone());
    }
    order
        .into_iter()
        .map(|k| {
            let mut rows = head_row(t);
            rows.extend(parts.remove(&k).unwrap_or_default());
            let name = if k.is_empty() { "empty".to_string() } else { k };
            (name.clone(), with_rows(t, rows))
        })
        .collect()
}

/// Split into tables of at most `size` data rows; each keeps the header.
pub fn split_rows(t: &Table, size: usize) -> Vec<Table> {
    body(t)
        .chunks(size.max(1))
        .map(|chunk| {
            let mut rows = head_row(t);
            rows.extend(chunk.iter().cloned());
            with_rows(t, rows)
        })
        .collect()
}
