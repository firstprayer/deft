//! iCalendar files: events to and from CSV.

use crate::lines::{property, unescape, unfold};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Event {
    pub summary: String,
    pub start: String,
    pub end: String,
    pub timezone: String,
    pub location: String,
    pub description: String,
    pub repeat: String,
    pub status: String,
    /// Dates a repeating event skips, joined with `; `.
    pub except: String,
}

const COLUMNS: [&str; 9] = [
    "summary",
    "start",
    "end",
    "timezone",
    "location",
    "description",
    "repeat",
    "status",
    "except",
];

/// Days since 1970 for a calendar date.
fn days_from(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let year = y - era * 400;
    let day = (153 * ((m + 9) % 12) + 2) / 5 + d - 1;
    era * 146_097 + year * 365 + year / 4 - year / 100 + day - 719_468
}

/// The calendar date of a day since 1970.
fn date_from(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let (d, m) = (
        doy - (153 * mp + 2) / 5 + 1,
        if mp < 10 { mp + 3 } else { mp - 9 },
    );
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// The digits of a date or date-time.
fn digits(value: &str) -> String {
    value.chars().filter(char::is_ascii_digit).collect()
}

/// Move `2026-11-06` or `2026-11-06 09:00` on by some seconds.
fn shifted(value: &str, seconds: i64) -> Option<String> {
    let all = digits(value);
    let part = |r: std::ops::Range<usize>| all.get(r)?.parse::<i64>().ok();
    let day = days_from(part(0..4)?, part(4..6)?, part(6..8)?);
    let clock = if all.len() >= 12 {
        part(8..10)? * 3600 + part(10..12)? * 60
    } else {
        0
    };
    let total = day * 86_400 + clock + seconds;
    let (y, m, d) = date_from(total.div_euclid(86_400));
    let rest = total.rem_euclid(86_400);
    Some(if all.len() >= 12 {
        format!(
            "{y:04}-{m:02}-{d:02} {:02}:{:02}",
            rest / 3600,
            rest % 3600 / 60
        )
    } else {
        format!("{y:04}-{m:02}-{d:02}")
    })
}

/// `P1DT2H30M` in seconds. A negative duration has no use as an event length.
fn duration(value: &str) -> Option<i64> {
    let body = value.strip_prefix('+').unwrap_or(value).strip_prefix('P')?;
    let (mut total, mut number) = (0, String::new());
    for c in body.chars() {
        match c {
            '0'..='9' => number.push(c),
            'T' => {}
            'W' | 'D' | 'H' | 'M' | 'S' => {
                let unit = match c {
                    'W' => 604_800,
                    'D' => 86_400,
                    'H' => 3600,
                    'M' => 60,
                    _ => 1,
                };
                total += number.parse::<i64>().ok()? * unit;
                number.clear();
            }
            _ => return None,
        }
    }
    Some(total)
}

/// The last day or moment of an event from its length. An all-day event's
/// last day is the day before the one the length reaches.
fn end_after(start: &str, seconds: i64) -> Option<String> {
    let all_day = digits(start).len() == 8;
    shifted(start, if all_day { seconds - 86_400 } else { seconds })
}

/// `20261106T090000Z` as ("2026-11-06 09:00", "UTC"). A date alone stays a date.
fn readable(value: &str) -> (String, &'static str) {
    let (body, utc) = match value.strip_suffix('Z') {
        Some(body) => (body, true),
        None => (value, false),
    };
    let digits = |r: std::ops::Range<usize>| body.get(r).unwrap_or("");
    let date = format!("{}-{}-{}", digits(0..4), digits(4..6), digits(6..8));
    if body.len() < 13 {
        return (date, "");
    }
    let text = format!("{date} {}:{}", digits(9..11), digits(11..13));
    (text, if utc { "UTC" } else { "" })
}

pub fn parse(text: &str) -> Vec<Event> {
    let mut events = Vec::new();
    let mut current: Option<Event> = None;
    let mut length = None;
    // A component inside an event, such as VALARM, with properties of its own.
    let mut inner: Option<String> = None;
    for line in unfold(text) {
        let Some((name, params, value)) = property(&line) else {
            continue;
        };
        let upper = value.to_ascii_uppercase();
        if let Some(open) = &inner {
            if name == "END" && upper == *open {
                inner = None;
            }
            continue;
        }
        match (name.as_str(), upper.as_str()) {
            ("BEGIN", "VEVENT") => {
                current = Some(Event::default());
                length = None;
            }
            ("BEGIN", _) if current.is_some() => {
                inner = Some(upper);
                continue;
            }
            ("END", "VEVENT") => {
                if let Some(e) = current.as_mut()
                    && e.end.is_empty()
                    && let Some(end) = length.and_then(|s| end_after(&e.start, s))
                {
                    e.end = end;
                }
                events.extend(current.take());
            }
            _ => {}
        }
        let Some(e) = current.as_mut() else {
            continue;
        };
        let zone = params.iter().find_map(|p| p.strip_prefix("TZID="));
        match name.as_str() {
            "SUMMARY" => e.summary = unescape(value),
            "LOCATION" => e.location = unescape(value),
            "DESCRIPTION" => e.description = unescape(value),
            "RRULE" => e.repeat = value.to_string(),
            "STATUS" => e.status = value.to_string(),
            "DURATION" => length = duration(value),
            "EXDATE" => {
                for date in value.split(',') {
                    if !e.except.is_empty() {
                        e.except.push_str("; ");
                    }
                    e.except.push_str(&readable(date).0);
                }
            }
            "DTSTART" => {
                let (when, utc) = readable(value);
                e.start = when;
                e.timezone = zone.unwrap_or(utc).to_string();
            }
            // An all-day DTEND is the day after the last; a table shows the last.
            "DTEND" => {
                let end = readable(value).0;
                e.end = if digits(&end).len() == 8 {
                    shifted(&end, -86_400).unwrap_or(end)
                } else {
                    end
                };
            }
            _ => {}
        }
    }
    events.sort_by(|a, b| a.start.cmp(&b.start));
    events
}

pub fn to_csv(events: &[Event]) -> Result<Vec<u8>, String> {
    let mut w = csv::Writer::from_writer(Vec::new());
    w.write_record(COLUMNS).map_err(|e| e.to_string())?;
    for e in events {
        w.write_record([
            &e.summary,
            &e.start,
            &e.end,
            &e.timezone,
            &e.location,
            &e.description,
            &e.repeat,
            &e.status,
            &e.except,
        ])
        .map_err(|e| e.to_string())?;
    }
    w.into_inner().map_err(|e| e.to_string())
}

/// `9:30 PM` as `21:30`. What it cannot read stays, for `when` to refuse.
fn clock(text: &str) -> String {
    let lower = text.trim().to_ascii_lowercase();
    let (body, half) = match (lower.strip_suffix("am"), lower.strip_suffix("pm")) {
        (Some(b), _) => (b, Some(false)),
        (_, Some(b)) => (b, Some(true)),
        _ => (lower.as_str(), None),
    };
    let mut parts = body.trim().split(':');
    let hour = parts.next().and_then(|h| h.parse::<u32>().ok());
    let minute = parts.next().map_or(Some(0), |m| m.parse::<u32>().ok());
    let (Some(hour), Some(minute)) = (hour, minute) else {
        return text.to_string();
    };
    let hour = match half {
        Some(true) if hour < 12 => hour + 12,
        Some(false) if hour == 12 => 0,
        _ => hour,
    };
    format!("{hour:02}:{minute:02}")
}

/// A date with its time from a column of its own, unless it has one already.
fn with_time(date: &str, time: &str) -> String {
    if time.is_empty() || digits(date).len() > 8 {
        date.to_string()
    } else {
        format!("{date} {}", clock(time))
    }
}

pub fn from_csv(bytes: &[u8]) -> Result<Vec<Event>, String> {
    let mut r = csv::Reader::from_reader(bytes);
    let headers: Vec<String> = r
        .headers()
        .map_err(|e| e.to_string())?
        .iter()
        .map(|h| h.trim().to_ascii_lowercase())
        .collect();
    let find = |names: &[&str]| headers.iter().position(|h| names.contains(&h.as_str()));
    let summary =
        find(&["summary", "title", "subject", "event"]).ok_or("the CSV needs a summary column")?;
    let start = find(&["start", "start date", "date"])
        .or_else(|| find(&["start time"]))
        .ok_or("the CSV needs a start column")?;
    let start_time = find(&["start time", "time"]).filter(|t| *t != start);
    let end_time = find(&["end time"]);
    let all_day = find(&["all day event", "all day"]);
    let columns = [
        find(&["end", "end date"]),
        find(&["timezone", "time zone"]),
        find(&["location", "place"]),
        find(&["description", "notes"]),
        find(&["repeat", "rrule"]),
        find(&["status"]),
        find(&["except", "exdate"]),
    ];
    let mut events = Vec::new();
    for row in r.records() {
        let row = row.map_err(|e| e.to_string())?;
        let cell = |i: Option<usize>| i.and_then(|i| row.get(i)).unwrap_or("").trim().to_string();
        let [end, timezone, location, description, repeat, status, except] = columns;
        let timed = !cell(all_day).eq_ignore_ascii_case("true");
        let (day, last_day) = (cell(Some(start)), cell(end));
        let time = |i| if timed { cell(i) } else { String::new() };
        let finish = time(end_time);
        // An end time alone falls on the day the event starts.
        let last_day = if last_day.is_empty() && !finish.is_empty() {
            day.clone()
        } else {
            last_day
        };
        events.push(Event {
            summary: cell(Some(summary)),
            start: with_time(&day, &time(start_time)),
            end: with_time(&last_day, &finish),
            timezone: cell(timezone),
            location: cell(location),
            description: cell(description),
            repeat: cell(repeat),
            status: cell(status),
            except: cell(except),
        });
    }
    Ok(events)
}

fn escape(value: &str) -> String {
    value
        .replace('\\', r"\\")
        .replace('\n', r"\n")
        .replace(',', r"\,")
        .replace(';', r"\;")
}

/// Whether digits read as a real date, with a real time if they have one.
/// US dates fail here: 11/06/2026 reads as month 20 of year 1106.
fn is_real(digits: &str) -> bool {
    if !matches!(digits.len(), 8 | 12 | 14) {
        return false;
    }
    let part = |r: std::ops::Range<usize>| digits.get(r).and_then(|p| p.parse::<i64>().ok());
    let (Some(y), Some(m), Some(d)) = (part(0..4), part(4..6), part(6..8)) else {
        return false;
    };
    if !(1..=12).contains(&m) || d < 1 {
        return false;
    }
    let (next_y, next_m) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    let month_days = days_from(next_y, next_m, 1) - days_from(y, m, 1);
    d <= month_days
        && part(8..10).is_none_or(|h| h < 24)
        && part(10..12).is_none_or(|min| min < 60)
        && part(12..14).is_none_or(|s| s < 61)
}

/// The DTEND line of an event, which must match DTSTART in kind. An all-day
/// DTEND is exclusive, so it is the day after the last.
fn end_line(e: &Event) -> Result<Option<String>, String> {
    if e.end.is_empty() {
        return Ok(None);
    }
    let (start, end) = (digits(&e.start), digits(&e.end));
    if start.len() == 8 {
        let last = e.end.split_whitespace().next().unwrap_or(&e.end);
        when("DTEND", last, "")?;
        let next = shifted(last, 86_400).unwrap_or_else(|| last.to_string());
        return when("DTEND", &next, &e.timezone).map(Some);
    }
    // A date alone ends at the time of day the event started.
    let end = if end.len() == 8 {
        format!("{end}{}", &start[8..])
    } else {
        end
    };
    let line = when("DTEND", &end, &e.timezone)?;
    let padded = |d: &str| format!("{d:0<14}");
    match padded(&end).cmp(&padded(&start)) {
        std::cmp::Ordering::Greater => Ok(Some(line)),
        std::cmp::Ordering::Equal => Ok(None),
        std::cmp::Ordering::Less => Err(format!(
            "{:?} ends before it starts: {} to {}",
            e.summary, e.start, e.end
        )),
    }
}

/// A UID that depends on the event, so that two files never collide and
/// importing the same event again updates it.
fn uid(e: &Event, repeat: usize) -> String {
    let text = format!("{}\0{}\0{repeat}", e.summary, e.start);
    let hash = text.bytes().fold(14_695_981_039_346_656_037_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(1_099_511_628_211)
    });
    format!("{hash:016x}@deftcli.com")
}

/// `2026-11-06 09:00` as a DTSTART or DTEND line.
fn when(name: &str, value: &str, timezone: &str) -> Result<String, String> {
    let digits = digits(value);
    if !is_real(&digits) {
        return Err(format!("{value:?} is not a date such as 2026-11-06 09:00"));
    }
    match digits.len() {
        8 => Ok(format!("{name};VALUE=DATE:{digits}\r\n")),
        12 | 14 => {
            let stamp = format!("{}T{:0<6}", &digits[..8], &digits[8..]);
            Ok(match timezone {
                "UTC" => format!("{name}:{stamp}Z\r\n"),
                "" => format!("{name}:{stamp}\r\n"),
                zone => format!("{name};TZID={zone}:{stamp}\r\n"),
            })
        }
        _ => Err(format!("{value:?} is not a date such as 2026-11-06 09:00")),
    }
}

pub fn write(events: &[Event]) -> Result<String, String> {
    let mut s = String::from("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//deft//deft//EN\r\n");
    let mut seen = std::collections::HashMap::new();
    for e in events {
        s.push_str("BEGIN:VEVENT\r\n");
        let repeat: &mut usize = seen.entry((&e.summary, &e.start)).or_default();
        s.push_str(&format!("UID:{}\r\n", uid(e, *repeat)));
        *repeat += 1;
        // Fixed, so the same table always gives the same file.
        s.push_str("DTSTAMP:20000101T000000Z\r\n");
        s.push_str(&when("DTSTART", &e.start, &e.timezone)?);
        if let Some(line) = end_line(e)? {
            s.push_str(&line);
        }
        s.push_str(&format!("SUMMARY:{}\r\n", escape(&e.summary)));
        if !e.location.is_empty() {
            s.push_str(&format!("LOCATION:{}\r\n", escape(&e.location)));
        }
        if !e.description.is_empty() {
            s.push_str(&format!("DESCRIPTION:{}\r\n", escape(&e.description)));
        }
        if !e.repeat.is_empty() {
            s.push_str(&format!("RRULE:{}\r\n", e.repeat));
        }
        if !e.status.is_empty() {
            s.push_str(&format!("STATUS:{}\r\n", e.status));
        }
        for date in e.except.split(';').map(str::trim).filter(|d| !d.is_empty()) {
            s.push_str(&when("EXDATE", date, &e.timezone)?);
        }
        s.push_str("END:VEVENT\r\n");
    }
    s.push_str("END:VCALENDAR\r\n");
    Ok(s)
}
