//! Shared option value types. Every command uses these parsers, so a page
//! range or a duration is written the same way everywhere.

use crate::error::{Result, bad_value};
use std::path::PathBuf;

/// The syntax an option accepts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Flag,
    Int { min: i64, max: i64 },
    Float { min: f64, max: f64 },
    Text,
    Choice(&'static [&'static str]),
    Pages,
    Duration,
    Times,
    Size,
    Length,
    Bytes,
    Color,
    Position,
    Path,
}

impl Kind {
    /// Placeholder shown in help, e.g. `<pages>`.
    pub fn placeholder(&self) -> String {
        match self {
            Kind::Flag => String::new(),
            Kind::Int { .. } => "<n>".into(),
            Kind::Float { .. } => "<number>".into(),
            Kind::Text => "<text>".into(),
            Kind::Choice(c) => format!("<{}>", c.join("|")),
            Kind::Pages => "<pages>".into(),
            Kind::Duration => "<time>".into(),
            Kind::Times => "<times>".into(),
            Kind::Size => "<size>".into(),
            Kind::Length => "<length>".into(),
            Kind::Bytes => "<bytes>".into(),
            Kind::Color => "<color>".into(),
            Kind::Position => "<position>".into(),
            Kind::Path => "<path>".into(),
        }
    }

    /// Name used to check that one option name always has one kind.
    pub fn family(&self) -> &'static str {
        match self {
            Kind::Flag => "flag",
            Kind::Int { .. } => "int",
            Kind::Float { .. } => "float",
            Kind::Text => "text",
            Kind::Choice(_) => "choice",
            Kind::Pages => "pages",
            Kind::Duration => "time",
            Kind::Times => "times",
            Kind::Size => "size",
            Kind::Length => "length",
            Kind::Bytes => "bytes",
            Kind::Color => "color",
            Kind::Position => "position",
            Kind::Path => "path",
        }
    }

    /// JSON Schema fragment for this kind.
    pub fn schema(&self) -> serde_json::Value {
        use serde_json::json;
        match self {
            Kind::Flag => json!({"type": "boolean"}),
            Kind::Int { min, max } => json!({"type": "integer", "minimum": min, "maximum": max}),
            Kind::Float { min, max } => json!({"type": "number", "minimum": min, "maximum": max}),
            Kind::Choice(c) => json!({"type": "string", "enum": c}),
            other => json!({"type": "string", "description": syntax_help(other)}),
        }
    }

    pub fn parse(&self, option: &str, raw: &str) -> Result<Value> {
        // Free text keeps its spaces: one may be the very thing to look for.
        let text = raw;
        let raw = raw.trim();
        Ok(match self {
            Kind::Flag => Value::Flag(
                parse_bool(raw).ok_or_else(|| bad_value(option, "expected true or false"))?,
            ),
            Kind::Int { min, max } => {
                let n: i64 = raw
                    .parse()
                    .map_err(|_| bad_value(option, format!("{raw:?} is not a whole number")))?;
                if n < *min || n > *max {
                    return Err(bad_value(option, format!("{n} is outside {min}..{max}")));
                }
                Value::Int(n)
            }
            Kind::Float { min, max } => {
                let n: f64 = raw
                    .parse()
                    .map_err(|_| bad_value(option, format!("{raw:?} is not a number")))?;
                if !(n >= *min && n <= *max) {
                    return Err(bad_value(option, format!("{n} is outside {min}..{max}")));
                }
                Value::Float(n)
            }
            Kind::Text => Value::Text(text.to_string()),
            Kind::Choice(choices) => {
                let lower = raw.to_ascii_lowercase();
                match choices.iter().find(|c| **c == lower) {
                    Some(c) => Value::Text((*c).to_string()),
                    None => {
                        return Err(bad_value(
                            option,
                            format!("{raw:?} is not one of {}", choices.join(", ")),
                        ));
                    }
                }
            }
            Kind::Pages => Value::Pages(PageSet::parse(raw).map_err(|m| bad_value(option, m))?),
            Kind::Duration => {
                Value::Seconds(parse_duration(raw).map_err(|m| bad_value(option, m))?)
            }
            Kind::Times => Value::Times(
                raw.split(',')
                    .map(|t| parse_duration(t.trim()))
                    .collect::<std::result::Result<_, _>>()
                    .map_err(|m| bad_value(option, m))?,
            ),
            Kind::Size => Value::Size(SizeSpec::parse(raw).map_err(|m| bad_value(option, m))?),
            Kind::Length => Value::Points(parse_length(raw).map_err(|m| bad_value(option, m))?),
            Kind::Bytes => Value::Bytes(parse_bytes(raw).map_err(|m| bad_value(option, m))?),
            Kind::Color => Value::Color(parse_color(raw).map_err(|m| bad_value(option, m))?),
            Kind::Position => {
                Value::Position(Position::parse(raw).map_err(|m| bad_value(option, m))?)
            }
            Kind::Path => Value::Path(PathBuf::from(raw)),
        })
    }
}

/// One-line syntax reference used by `deft help values` and in schemas.
pub fn syntax_help(kind: &Kind) -> &'static str {
    match kind {
        Kind::Pages => "1-3,5,8- (8 to end), last, -2 (second to last), odd, even, all",
        Kind::Duration => "90, 1:30, 01:02:03.5, 1m30s, 500ms",
        Kind::Times => "comma-separated times: 0:05,0:10,1m",
        Kind::Size => "1920x1080, 800x (width, keep ratio), x600 (height), 50%",
        Kind::Length => "10mm, 1cm, 0.5in, 36pt, 20px",
        Kind::Bytes => "500KB, 1.5MB, 2GB (1KB = 1024 bytes)",
        Kind::Color => "#ff0000, #f00, #ff000080 (with alpha), red, black, white, gray",
        Kind::Position => {
            "top-left, top, top-right, left, center, right, bottom-left, bottom, bottom-right"
        }
        Kind::Path => "a file or directory path",
        Kind::Text => "free text",
        _ => "",
    }
}

/// A parsed option value.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Flag(bool),
    Int(i64),
    Float(f64),
    Text(String),
    Pages(PageSet),
    Seconds(f64),
    Times(Vec<f64>),
    Size(SizeSpec),
    Points(f64),
    Bytes(u64),
    Color([u8; 4]),
    Position(Position),
    Path(PathBuf),
}

impl Value {
    pub fn to_json(&self) -> serde_json::Value {
        use serde_json::json;
        match self {
            Value::Flag(b) => json!(b),
            Value::Int(n) => json!(n),
            Value::Float(n) => json!(n),
            Value::Text(s) => json!(s),
            Value::Pages(p) => json!(p.source),
            Value::Seconds(s) => json!(s),
            Value::Times(t) => json!(t),
            Value::Size(s) => json!(s.source),
            Value::Points(p) => json!(p),
            Value::Bytes(b) => json!(b),
            Value::Color(c) => json!(format!("#{:02x}{:02x}{:02x}{:02x}", c[0], c[1], c[2], c[3])),
            Value::Position(p) => json!(p.name()),
            Value::Path(p) => json!(p.display().to_string()),
        }
    }
}

fn parse_bool(raw: &str) -> Option<bool> {
    match raw.to_ascii_lowercase().as_str() {
        "" | "true" | "yes" | "1" | "on" => Some(true),
        "false" | "no" | "0" | "off" => Some(false),
        _ => None,
    }
}

/// Normalise a rotation option to 90, 180 or 270 (0 stays 0).
pub fn right_angle(degrees: f64) -> Result<i64> {
    let d = degrees.round() as i64;
    if (degrees - d as f64).abs() > f64::EPSILON || d % 90 != 0 {
        return Err(bad_value(
            "angle",
            format!("{degrees} is not a multiple of 90"),
        ));
    }
    Ok(d.rem_euclid(360))
}

// ---------------------------------------------------------------- pages

#[derive(Clone, Copy, Debug, PartialEq)]
enum Bound {
    Abs(usize),
    FromEnd(usize),
}

#[derive(Clone, Debug, PartialEq)]
enum PageItem {
    Range(Option<Bound>, Option<Bound>),
    Odd,
    Even,
}

/// A page selection such as `1-3,5,8-`. Resolved against a page count.
#[derive(Clone, Debug, PartialEq)]
pub struct PageSet {
    items: Vec<PageItem>,
    pub source: String,
}

impl PageSet {
    pub fn all() -> PageSet {
        PageSet {
            items: vec![PageItem::Range(None, None)],
            source: "all".into(),
        }
    }

    pub fn parse(raw: &str) -> std::result::Result<PageSet, String> {
        let bound = |s: &str| -> std::result::Result<Bound, String> {
            let s = s.trim();
            if s.eq_ignore_ascii_case("last") || s.eq_ignore_ascii_case("end") {
                return Ok(Bound::FromEnd(1));
            }
            if let Some(rest) = s.strip_prefix('-') {
                let n: usize = rest.parse().map_err(|_| format!("bad page {s:?}"))?;
                return if n == 0 {
                    Err("page -0 does not exist".into())
                } else {
                    Ok(Bound::FromEnd(n))
                };
            }
            let n: usize = s.parse().map_err(|_| format!("bad page {s:?}"))?;
            if n == 0 {
                Err("pages start at 1".into())
            } else {
                Ok(Bound::Abs(n))
            }
        };
        let mut items = Vec::new();
        for part in raw.split(',').map(str::trim).filter(|p| !p.is_empty()) {
            let lower = part.to_ascii_lowercase();
            match lower.as_str() {
                "all" | "*" => items.push(PageItem::Range(None, None)),
                "odd" => items.push(PageItem::Odd),
                "even" => items.push(PageItem::Even),
                _ if part.starts_with('-') && part[1..].chars().all(|c| c.is_ascii_digit()) => {
                    let b = bound(part)?;
                    items.push(PageItem::Range(Some(b), Some(b)));
                }
                _ => match part.split_once('-') {
                    Some((a, b)) => {
                        let start = if a.trim().is_empty() {
                            None
                        } else {
                            Some(bound(a)?)
                        };
                        let end = if b.trim().is_empty() {
                            None
                        } else {
                            Some(bound(b)?)
                        };
                        items.push(PageItem::Range(start, end));
                    }
                    None => {
                        let b = bound(part)?;
                        items.push(PageItem::Range(Some(b), Some(b)));
                    }
                },
            }
        }
        if items.is_empty() {
            return Err("empty page selection".into());
        }
        Ok(PageSet {
            items,
            source: raw.to_string(),
        })
    }

    /// Zero-based page indexes in the order written. Errors on pages past the end.
    pub fn resolve(&self, count: usize) -> std::result::Result<Vec<usize>, String> {
        let abs = |b: Bound| -> std::result::Result<usize, String> {
            match b {
                Bound::Abs(n) if n <= count => Ok(n - 1),
                Bound::Abs(n) => Err(format!(
                    "page {n} does not exist (the document has {count})"
                )),
                Bound::FromEnd(n) if n <= count => Ok(count - n),
                Bound::FromEnd(n) => Err(format!(
                    "page -{n} does not exist (the document has {count})"
                )),
            }
        };
        let mut out = Vec::new();
        for item in &self.items {
            match item {
                PageItem::Odd => out.extend((0..count).step_by(2)),
                PageItem::Even => out.extend((1..count).step_by(2)),
                PageItem::Range(a, b) => {
                    if count == 0 {
                        continue;
                    }
                    let start = a.map(abs).transpose()?.unwrap_or(0);
                    let end = b.map(abs).transpose()?.unwrap_or(count - 1);
                    if start <= end {
                        out.extend(start..=end);
                    } else {
                        out.extend((end..=start).rev());
                    }
                }
            }
        }
        Ok(out)
    }

    /// Unique, sorted indexes (for "apply to these pages" options).
    pub fn resolve_set(&self, count: usize) -> std::result::Result<Vec<usize>, String> {
        let mut v = self.resolve(count)?;
        v.sort_unstable();
        v.dedup();
        Ok(v)
    }
}

// ---------------------------------------------------------------- time

pub fn parse_duration(raw: &str) -> std::result::Result<f64, String> {
    let raw = raw.trim();
    let bad = || format!("bad time {raw:?}; use 90, 1:30, 01:02:03.5 or 1m30s");
    if raw.is_empty() {
        return Err(bad());
    }
    if raw.contains(':') {
        let parts: Vec<&str> = raw.split(':').collect();
        if parts.len() > 3 {
            return Err(bad());
        }
        let mut total = 0.0;
        for p in &parts {
            let v: f64 = p.parse().map_err(|_| bad())?;
            if v < 0.0 {
                return Err(bad());
            }
            total = total * 60.0 + v;
        }
        return Ok(total);
    }
    if let Ok(v) = raw.parse::<f64>() {
        return if v >= 0.0 { Ok(v) } else { Err(bad()) };
    }
    // Unit form: 1h2m3.5s, 500ms
    let mut total = 0.0;
    let mut num = String::new();
    let chars: Vec<char> = raw.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_digit() || c == '.' {
            num.push(c);
            i += 1;
            continue;
        }
        let v: f64 = num.parse().map_err(|_| bad())?;
        num.clear();
        if c == 'm' && chars.get(i + 1) == Some(&'s') {
            total += v / 1000.0;
            i += 2;
            continue;
        }
        total += match c {
            'h' => v * 3600.0,
            'm' => v * 60.0,
            's' => v,
            _ => return Err(bad()),
        };
        i += 1;
    }
    if !num.is_empty() {
        return Err(bad());
    }
    Ok(total)
}

/// Format seconds as `h:mm:ss.mmm` (or `m:ss`) for display.
pub fn format_duration(secs: f64) -> String {
    let total_ms = (secs * 1000.0).round() as u64;
    let (h, m, s, ms) = (
        total_ms / 3_600_000,
        total_ms / 60_000 % 60,
        total_ms / 1000 % 60,
        total_ms % 1000,
    );
    let frac = if ms > 0 {
        format!(".{ms:03}")
    } else {
        String::new()
    };
    if h > 0 {
        format!("{h}:{m:02}:{s:02}{frac}")
    } else {
        format!("{m}:{s:02}{frac}")
    }
}

// ---------------------------------------------------------------- size

#[derive(Clone, Debug, PartialEq)]
pub struct SizeSpec {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub percent: Option<f64>,
    pub source: String,
}

impl SizeSpec {
    pub fn parse(raw: &str) -> std::result::Result<SizeSpec, String> {
        let raw = raw.trim();
        let bad = || format!("bad size {raw:?}; use 1920x1080, 800x, x600 or 50%");
        let source = raw.to_string();
        if let Some(p) = raw.strip_suffix('%') {
            let v: f64 = p.parse().map_err(|_| bad())?;
            if !(v > 0.0 && v <= 1000.0) {
                return Err(bad());
            }
            return Ok(SizeSpec {
                width: None,
                height: None,
                percent: Some(v),
                source,
            });
        }
        let lower = raw.to_ascii_lowercase();
        let (w, h) = match lower.split_once(['x', '*']) {
            Some((w, h)) => (w.trim(), h.trim()),
            None => (lower.as_str(), ""),
        };
        let dim = |s: &str| -> std::result::Result<Option<u32>, String> {
            let s = s.trim_end_matches("px");
            if s.is_empty() {
                return Ok(None);
            }
            match s.parse::<u32>() {
                Ok(0) | Err(_) => Err(bad()),
                Ok(v) if v > 100_000 => Err(bad()),
                Ok(v) => Ok(Some(v)),
            }
        };
        let (width, height) = (dim(w)?, dim(h)?);
        if width.is_none() && height.is_none() {
            return Err(bad());
        }
        Ok(SizeSpec {
            width,
            height,
            percent: None,
            source,
        })
    }

    /// Target dimensions for a source of `w` x `h`, keeping the aspect ratio
    /// when only one side is given. With both sides given the box is returned
    /// as is; fitting is the caller's business.
    pub fn target(&self, w: u32, h: u32) -> (u32, u32) {
        if let Some(p) = self.percent {
            let f = p / 100.0;
            return (
                ((w as f64 * f).round() as u32).max(1),
                ((h as f64 * f).round() as u32).max(1),
            );
        }
        match (self.width, self.height) {
            (Some(tw), Some(th)) => (tw, th),
            (Some(tw), None) => (
                tw,
                ((h as f64 * tw as f64 / w as f64).round() as u32).max(1),
            ),
            (None, Some(th)) => (
                ((w as f64 * th as f64 / h as f64).round() as u32).max(1),
                th,
            ),
            (None, None) => (w, h),
        }
    }

    pub fn is_box(&self) -> bool {
        self.width.is_some() && self.height.is_some()
    }
}

// ---------------------------------------------------------------- length, bytes, color

/// Length in PDF points (1/72 inch).
pub fn parse_length(raw: &str) -> std::result::Result<f64, String> {
    let raw = raw.trim().to_ascii_lowercase();
    let bad = || format!("bad length {raw:?}; use 10mm, 1cm, 0.5in, 36pt or 20px");
    let units: [(&str, f64); 5] = [
        ("mm", 72.0 / 25.4),
        ("cm", 72.0 / 2.54),
        ("in", 72.0),
        ("pt", 1.0),
        ("px", 0.75),
    ];
    for (u, f) in units {
        if let Some(n) = raw.strip_suffix(u) {
            let v: f64 = n.trim().parse().map_err(|_| bad())?;
            return if v >= 0.0 { Ok(v * f) } else { Err(bad()) };
        }
    }
    let v: f64 = raw.parse().map_err(|_| bad())?;
    if v >= 0.0 { Ok(v) } else { Err(bad()) }
}

pub fn parse_bytes(raw: &str) -> std::result::Result<u64, String> {
    let raw = raw.trim().to_ascii_uppercase().replace(' ', "");
    let bad = || format!("bad size {raw:?}; use 500KB, 1.5MB or 2GB");
    let units: [(&str, f64); 8] = [
        ("KIB", 1024.0),
        ("MIB", 1048576.0),
        ("GIB", 1073741824.0),
        ("KB", 1024.0),
        ("MB", 1048576.0),
        ("GB", 1073741824.0),
        ("K", 1024.0),
        ("M", 1048576.0),
    ];
    for (u, f) in units {
        if let Some(n) = raw.strip_suffix(u) {
            let v: f64 = n.parse().map_err(|_| bad())?;
            return if v > 0.0 {
                Ok((v * f) as u64)
            } else {
                Err(bad())
            };
        }
    }
    let n = raw.strip_suffix('B').unwrap_or(&raw);
    n.parse::<u64>().map_err(|_| bad())
}

/// Human file size: 1.2MB, 340KB, 12B.
pub fn format_bytes(n: u64) -> String {
    let f = n as f64;
    if f >= 1073741824.0 {
        format!("{:.1}GB", f / 1073741824.0)
    } else if f >= 1048576.0 {
        format!("{:.1}MB", f / 1048576.0)
    } else if f >= 1024.0 {
        format!("{:.0}KB", f / 1024.0)
    } else {
        format!("{n}B")
    }
}

pub fn parse_color(raw: &str) -> std::result::Result<[u8; 4], String> {
    let raw = raw.trim().to_ascii_lowercase();
    let bad = || format!("bad color {raw:?}; use #ff0000, #f00, #ff000080 or a name like red");
    let named = match raw.as_str() {
        "black" => Some([0, 0, 0, 255]),
        "white" => Some([255, 255, 255, 255]),
        "red" => Some([220, 38, 38, 255]),
        "green" => Some([22, 163, 74, 255]),
        "blue" => Some([37, 99, 235, 255]),
        "gray" | "grey" => Some([128, 128, 128, 255]),
        "yellow" => Some([234, 179, 8, 255]),
        "orange" => Some([234, 88, 12, 255]),
        "purple" => Some([147, 51, 234, 255]),
        "transparent" => Some([0, 0, 0, 0]),
        _ => None,
    };
    if let Some(c) = named {
        return Ok(c);
    }
    let hex = raw.strip_prefix('#').ok_or_else(bad)?;
    let digits: Vec<u8> = hex
        .chars()
        .map(|c| c.to_digit(16).map(|d| d as u8))
        .collect::<Option<_>>()
        .ok_or_else(bad)?;
    match digits.len() {
        3 => Ok([digits[0] * 17, digits[1] * 17, digits[2] * 17, 255]),
        6 | 8 => {
            let byte = |i: usize| digits[i] * 16 + digits[i + 1];
            Ok([
                byte(0),
                byte(2),
                byte(4),
                if digits.len() == 8 { byte(6) } else { 255 },
            ])
        }
        _ => Err(bad()),
    }
}

/// Colour from `#rrggbb` or a name, as RGB. For tests and tools.
pub fn parse_color_hex(raw: &str) -> std::result::Result<[u8; 3], String> {
    parse_color(raw).map(|c| [c[0], c[1], c[2]])
}

// ---------------------------------------------------------------- position

/// A nine-grid anchor. `x` and `y` are 0, 0.5 or 1, measured from the top-left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

const POSITIONS: [(&str, f64, f64); 9] = [
    ("top-left", 0.0, 0.0),
    ("top", 0.5, 0.0),
    ("top-right", 1.0, 0.0),
    ("left", 0.0, 0.5),
    ("center", 0.5, 0.5),
    ("right", 1.0, 0.5),
    ("bottom-left", 0.0, 1.0),
    ("bottom", 0.5, 1.0),
    ("bottom-right", 1.0, 1.0),
];

impl Position {
    pub fn parse(raw: &str) -> std::result::Result<Position, String> {
        let norm = raw.trim().to_ascii_lowercase().replace(['_', ' '], "-");
        let norm = match norm.as_str() {
            "centre" | "middle" => "center".to_string(),
            "top-center" => "top".to_string(),
            "bottom-center" => "bottom".to_string(),
            other => other.to_string(),
        };
        POSITIONS
            .iter()
            .find(|(n, _, _)| *n == norm)
            .map(|(_, x, y)| Position { x: *x, y: *y })
            .ok_or_else(|| {
                format!(
                    "bad position {raw:?}; use {}",
                    POSITIONS.map(|p| p.0).join(", ")
                )
            })
    }

    pub fn name(&self) -> &'static str {
        POSITIONS
            .iter()
            .find(|(_, x, y)| *x == self.x && *y == self.y)
            .map(|p| p.0)
            .unwrap_or("center")
    }
}

/// Undo URL percent-encoding, as in `my%20photo.png`. A `%` that starts no
/// valid escape stays as it is.
pub fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok())
            .and_then(|h| u8::from_str_radix(h, 16).ok());
        match (bytes[i], hex) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
#[path = "value_tests.rs"]
mod tests;
