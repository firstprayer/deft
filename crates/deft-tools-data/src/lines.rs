//! The line format vCard and iCalendar share.

/// Joins folded lines. A line that starts with a space continues the one before,
/// and a quoted-printable line ending in `=` continues on the next.
pub fn unfold(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        let last = lines.last_mut();
        match (line.strip_prefix([' ', '\t']), last) {
            (Some(rest), Some(previous)) => previous.push_str(rest),
            (_, Some(previous))
                if previous.ends_with('=')
                    && previous.to_ascii_uppercase().contains("QUOTED-PRINTABLE") =>
            {
                previous.pop();
                previous.push_str(line);
            }
            _ => lines.push(line.to_string()),
        }
    }
    lines
}

/// `item1.TEL;TYPE=CELL:+1` as ("TEL", ["TYPE=CELL"], "+1").
pub fn property(line: &str) -> Option<(String, Vec<String>, &str)> {
    let (head, value) = line.split_once(':')?;
    let mut parts = head.split(';');
    let name = parts.next()?;
    let name = name.rsplit('.').next().unwrap_or(name).to_ascii_uppercase();
    Some((name, parts.map(String::from).collect(), value))
}

pub fn unescape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n' | 'N') => out.push('\n'),
            Some(other) => out.push(other),
            None => {}
        }
    }
    out
}
