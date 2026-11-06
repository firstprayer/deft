//! Row filters such as `age > 30 and city = 'Oslo'`. The grammar is small on
//! purpose: comparisons joined by and, or, not and parentheses.

use crate::{Cell, Table, find_column};
use regex::Regex;

#[derive(Clone, Debug)]
pub enum Filter {
    And(Box<Filter>, Box<Filter>),
    Or(Box<Filter>, Box<Filter>),
    Not(Box<Filter>),
    Compare(usize, Op, Cell),
    Matches(usize, Regex),
    In(usize, Vec<Cell>),
    Empty(usize),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Op {
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Contains,
    Starts,
    Ends,
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Word(String),
    Quoted(String),
    Symbol(&'static str),
}

fn lex(s: &str) -> Result<Vec<Tok>, String> {
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    let mut out = Vec::new();
    while i < chars.len() {
        let c = chars[i];
        match c {
            c if c.is_whitespace() => i += 1,
            '\'' | '"' | '`' => {
                let mut text = String::new();
                i += 1;
                loop {
                    let Some(&d) = chars.get(i) else {
                        return Err(format!("missing closing {c}"));
                    };
                    i += 1;
                    if d == c {
                        // A doubled quote stands for itself.
                        if chars.get(i) == Some(&c) {
                            text.push(c);
                            i += 1;
                            continue;
                        }
                        break;
                    }
                    text.push(d);
                }
                out.push(Tok::Quoted(text));
            }
            '(' | ')' | ',' | '~' => {
                out.push(Tok::Symbol(match c {
                    '(' => "(",
                    ')' => ")",
                    ',' => ",",
                    _ => "~",
                }));
                i += 1;
            }
            '=' | '!' | '<' | '>' => {
                let next = chars.get(i + 1).copied();
                let sym = match (c, next) {
                    ('=', Some('=')) => ("=", 2),
                    ('=', _) => ("=", 1),
                    ('!', Some('=')) => ("!=", 2),
                    ('!', Some('~')) => ("!~", 2),
                    ('<', Some('=')) => ("<=", 2),
                    ('<', Some('>')) => ("!=", 2),
                    ('<', _) => ("<", 1),
                    ('>', Some('=')) => (">=", 2),
                    ('>', _) => (">", 1),
                    _ => return Err("expected != or !~".into()),
                };
                out.push(Tok::Symbol(sym.0));
                i += sym.1;
            }
            _ => {
                let start = i;
                while i < chars.len()
                    && !chars[i].is_whitespace()
                    && !"()'\"`,=!<>~".contains(chars[i])
                {
                    i += 1;
                }
                out.push(Tok::Word(chars[start..i].iter().collect()));
            }
        }
    }
    Ok(out)
}

struct Parser<'a> {
    toks: Vec<Tok>,
    at: usize,
    table: &'a Table,
}

impl Parser<'_> {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.at)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.at).cloned();
        self.at += 1;
        t
    }
    fn keyword(&mut self, word: &str) -> bool {
        let hit = matches!(self.peek(), Some(Tok::Word(w)) if w.eq_ignore_ascii_case(word));
        if hit {
            self.at += 1;
        }
        hit
    }
    fn symbol(&mut self, sym: &str) -> bool {
        let hit = matches!(self.peek(), Some(Tok::Symbol(s)) if *s == sym);
        if hit {
            self.at += 1;
        }
        hit
    }

    fn or(&mut self) -> Result<Filter, String> {
        let mut left = self.and()?;
        while self.keyword("or") {
            left = Filter::Or(Box::new(left), Box::new(self.and()?));
        }
        Ok(left)
    }
    fn and(&mut self) -> Result<Filter, String> {
        let mut left = self.not()?;
        while self.keyword("and") {
            left = Filter::And(Box::new(left), Box::new(self.not()?));
        }
        Ok(left)
    }
    fn not(&mut self) -> Result<Filter, String> {
        if self.keyword("not") {
            return Ok(Filter::Not(Box::new(self.not()?)));
        }
        if self.symbol("(") {
            let inner = self.or()?;
            if !self.symbol(")") {
                return Err("missing )".into());
            }
            return Ok(inner);
        }
        self.comparison()
    }
    fn value(&mut self) -> Result<Cell, String> {
        match self.next() {
            Some(Tok::Quoted(s)) => Ok(Cell::Text(s)),
            Some(Tok::Word(w)) => Ok(match w.parse::<f64>() {
                Ok(n) => Cell::Number(n),
                Err(_) if w.eq_ignore_ascii_case("true") => Cell::Bool(true),
                Err(_) if w.eq_ignore_ascii_case("false") => Cell::Bool(false),
                Err(_) => Cell::Text(w),
            }),
            other => Err(format!("expected a value, found {other:?}")),
        }
    }
    fn comparison(&mut self) -> Result<Filter, String> {
        let name = match self.next() {
            Some(Tok::Word(w) | Tok::Quoted(w)) => w,
            other => return Err(format!("expected a column name, found {other:?}")),
        };
        let col = find_column(self.table, &name)?;
        if self.keyword("is") {
            let negated = self.keyword("not");
            if !self.keyword("empty") {
                return Err("expected: is empty, is not empty".into());
            }
            let f = Filter::Empty(col);
            return Ok(if negated { Filter::Not(Box::new(f)) } else { f });
        }
        let negated = self.keyword("not");
        if self.keyword("in") {
            if !self.symbol("(") {
                return Err("expected ( after in".into());
            }
            let mut list = vec![self.value()?];
            while self.symbol(",") {
                list.push(self.value()?);
            }
            if !self.symbol(")") {
                return Err("missing )".into());
            }
            let f = Filter::In(col, list);
            return Ok(if negated { Filter::Not(Box::new(f)) } else { f });
        }
        let op = match self.next() {
            Some(Tok::Symbol("=")) => Op::Eq,
            Some(Tok::Symbol("!=")) => Op::Ne,
            Some(Tok::Symbol("<")) => Op::Lt,
            Some(Tok::Symbol("<=")) => Op::Le,
            Some(Tok::Symbol(">")) => Op::Gt,
            Some(Tok::Symbol(">=")) => Op::Ge,
            Some(Tok::Symbol(s @ ("~" | "!~"))) => {
                let Cell::Text(p) = self.value()? else {
                    return Err("~ needs a quoted pattern".into());
                };
                let re = Regex::new(&p).map_err(|e| format!("bad pattern: {e}"))?;
                let f = Filter::Matches(col, re);
                return Ok(if s == "!~" {
                    Filter::Not(Box::new(f))
                } else {
                    f
                });
            }
            Some(Tok::Word(w)) => match w.to_ascii_lowercase().as_str() {
                "contains" => Op::Contains,
                "startswith" => Op::Starts,
                "endswith" => Op::Ends,
                _ => return Err(format!("unknown comparison {w:?}")),
            },
            other => return Err(format!("expected a comparison, found {other:?}")),
        };
        let f = Filter::Compare(col, op, self.value()?);
        Ok(if negated { Filter::Not(Box::new(f)) } else { f })
    }
}

/// Parse a filter against a table's header row.
pub fn parse_filter(text: &str, table: &Table) -> Result<Filter, String> {
    let mut p = Parser {
        toks: lex(text)?,
        at: 0,
        table,
    };
    let f = p.or()?;
    match p.peek() {
        None => Ok(f),
        Some(t) => Err(format!("unexpected {t:?}")),
    }
}

impl Filter {
    pub fn matches(&self, row: &[Cell]) -> bool {
        let cell = |c: usize| row.get(c).unwrap_or(&Cell::Empty);
        match self {
            Filter::And(a, b) => a.matches(row) && b.matches(row),
            Filter::Or(a, b) => a.matches(row) || b.matches(row),
            Filter::Not(a) => !a.matches(row),
            Filter::Empty(c) => cell(*c).to_string().is_empty(),
            Filter::Matches(c, re) => re.is_match(&cell(*c).to_string()),
            Filter::In(c, list) => list.iter().any(|v| equal(cell(*c), v)),
            Filter::Compare(c, op, v) => compare(cell(*c), *op, v),
        }
    }
}

/// A CSV cell such as `007` is text, yet `id = 7` should still match numbers.
fn as_number(c: &Cell) -> Option<f64> {
    c.number().or_else(|| match c {
        Cell::Text(s) => s.trim().parse().ok(),
        _ => None,
    })
}

fn equal(a: &Cell, b: &Cell) -> bool {
    match (as_number(a), as_number(b)) {
        (Some(x), Some(y)) => x == y,
        _ => a.to_string().eq_ignore_ascii_case(&b.to_string()),
    }
}

fn compare(cell: &Cell, op: Op, value: &Cell) -> bool {
    use std::cmp::Ordering::{Equal, Greater, Less};
    let text = || cell.to_string().to_lowercase();
    let want = || value.to_string().to_lowercase();
    match op {
        Op::Eq => equal(cell, value),
        Op::Ne => !equal(cell, value),
        Op::Contains => text().contains(&want()),
        Op::Starts => text().starts_with(&want()),
        Op::Ends => text().ends_with(&want()),
        Op::Lt | Op::Le | Op::Gt | Op::Ge => {
            if cell.to_string().is_empty() {
                return false;
            }
            let order = match (as_number(cell), as_number(value)) {
                (Some(x), Some(y)) => x.partial_cmp(&y),
                _ => Some(cell.to_string().cmp(&value.to_string())),
            };
            matches!(
                (op, order),
                (Op::Lt, Some(Less))
                    | (Op::Le, Some(Less | Equal))
                    | (Op::Gt, Some(Greater))
                    | (Op::Ge, Some(Greater | Equal))
            )
        }
    }
}
