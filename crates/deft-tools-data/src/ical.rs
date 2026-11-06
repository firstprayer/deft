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
}

const COLUMNS: [&str; 7] = [
    "summary",
    "start",
    "end",
    "timezone",
    "location",
    "description",
    "repeat",
];

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
    for line in unfold(text) {
        let Some((name, params, value)) = property(&line) else {
            continue;
        };
        match (name.as_str(), value.to_ascii_uppercase().as_str()) {
            ("BEGIN", "VEVENT") => current = Some(Event::default()),
            ("END", "VEVENT") => events.extend(current.take()),
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
            "DTSTART" => {
                let (when, utc) = readable(value);
                e.start = when;
                e.timezone = zone.unwrap_or(utc).to_string();
            }
            "DTEND" => e.end = readable(value).0,
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
        ])
        .map_err(|e| e.to_string())?;
    }
    w.into_inner().map_err(|e| e.to_string())
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
    let start = find(&["start", "start date", "date"]).ok_or("the CSV needs a start column")?;
    let columns = [
        find(&["end", "end date"]),
        find(&["timezone", "time zone"]),
        find(&["location", "place"]),
        find(&["description", "notes"]),
        find(&["repeat", "rrule"]),
    ];
    let mut events = Vec::new();
    for row in r.records() {
        let row = row.map_err(|e| e.to_string())?;
        let cell = |i: Option<usize>| i.and_then(|i| row.get(i)).unwrap_or("").trim().to_string();
        let [end, timezone, location, description, repeat] = columns;
        events.push(Event {
            summary: cell(Some(summary)),
            start: cell(Some(start)),
            end: cell(end),
            timezone: cell(timezone),
            location: cell(location),
            description: cell(description),
            repeat: cell(repeat),
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

/// `2026-11-06 09:00` as a DTSTART or DTEND line.
fn when(name: &str, value: &str, timezone: &str) -> Result<String, String> {
    let digits: String = value.chars().filter(char::is_ascii_digit).collect();
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
    for (n, e) in events.iter().enumerate() {
        s.push_str("BEGIN:VEVENT\r\n");
        s.push_str(&format!("UID:{}@deftcli.com\r\n", n + 1));
        let start = when("DTSTART", &e.start, &e.timezone)?;
        s.push_str(
            &start
                .replace("DTSTART", "DTSTAMP")
                .replace(";VALUE=DATE", ""),
        );
        s.push_str(&start);
        if !e.end.is_empty() {
            s.push_str(&when("DTEND", &e.end, &e.timezone)?);
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
        s.push_str("END:VEVENT\r\n");
    }
    s.push_str("END:VCALENDAR\r\n");
    Ok(s)
}
