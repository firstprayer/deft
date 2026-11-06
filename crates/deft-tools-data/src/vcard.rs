//! vCard contact files: read 2.1 to 4.0, write 3.0, and move to and from CSV.

use crate::lines::{property, unescape, unfold};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Contact {
    pub name: String,
    pub organization: String,
    pub title: String,
    pub phones: Vec<String>,
    pub emails: Vec<String>,
    pub address: String,
    pub birthday: String,
    pub url: String,
    pub note: String,
}

const COLUMNS: [&str; 9] = [
    "name",
    "organization",
    "title",
    "phone",
    "email",
    "address",
    "birthday",
    "url",
    "note",
];

/// Quoted-printable, which older phones use for anything outside ASCII.
fn quoted_printable(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok());
        match (bytes[i], hex.and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (b'=', Some(byte)) => {
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

fn decoded(params: &[String], value: &str) -> String {
    let encoded = params
        .iter()
        .any(|p| p.to_ascii_uppercase().contains("QUOTED-PRINTABLE"));
    if encoded {
        quoted_printable(value)
    } else {
        unescape(value)
    }
}

/// `Family;Given;Middle;Prefix;Suffix` as a name people would write.
fn structured_name(value: &str) -> String {
    let mut parts = value.split(';');
    let family = parts.next().unwrap_or("");
    let given = parts.next().unwrap_or("");
    [given, family]
        .iter()
        .filter(|p| !p.is_empty())
        .copied()
        .collect::<Vec<_>>()
        .join(" ")
}

fn joined_parts(value: &str) -> String {
    value
        .split(';')
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn parse(text: &str) -> Vec<Contact> {
    let mut contacts = Vec::new();
    let mut current: Option<Contact> = None;
    for line in unfold(text) {
        let Some((name, params, value)) = property(&line) else {
            continue;
        };
        match (name.as_str(), value.to_ascii_uppercase().as_str()) {
            ("BEGIN", "VCARD") => current = Some(Contact::default()),
            ("END", "VCARD") => contacts.extend(current.take()),
            _ => {}
        }
        let Some(c) = current.as_mut() else {
            continue;
        };
        let text = decoded(&params, value);
        match name.as_str() {
            "FN" => c.name = text,
            "N" if c.name.is_empty() => c.name = structured_name(&text),
            "ORG" => c.organization = joined_parts(&text),
            "TITLE" => c.title = text,
            "TEL" => c.phones.push(text),
            "EMAIL" => c.emails.push(text),
            "ADR" if c.address.is_empty() => c.address = joined_parts(&text),
            "BDAY" => c.birthday = text,
            "URL" => c.url = text,
            "NOTE" => c.note = text,
            _ => {}
        }
    }
    contacts
}

pub fn to_csv(contacts: &[Contact]) -> Result<Vec<u8>, String> {
    let mut w = csv::Writer::from_writer(Vec::new());
    w.write_record(COLUMNS).map_err(|e| e.to_string())?;
    for c in contacts {
        let (phones, emails) = (c.phones.join("; "), c.emails.join("; "));
        w.write_record([
            &c.name,
            &c.organization,
            &c.title,
            &phones,
            &emails,
            &c.address,
            &c.birthday,
            &c.url,
            &c.note,
        ])
        .map_err(|e| e.to_string())?;
    }
    w.into_inner().map_err(|e| e.to_string())
}

fn list(value: &str) -> Vec<String> {
    value
        .split(';')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(String::from)
        .collect()
}

pub fn from_csv(bytes: &[u8]) -> Result<Vec<Contact>, String> {
    let mut r = csv::Reader::from_reader(bytes);
    let headers: Vec<String> = r
        .headers()
        .map_err(|e| e.to_string())?
        .iter()
        .map(|h| h.trim().to_ascii_lowercase())
        .collect();
    let find = |names: &[&str]| headers.iter().position(|h| names.contains(&h.as_str()));
    let name = find(&["name", "full name", "fn"]).ok_or("the CSV needs a name column")?;
    let columns = [
        find(&["organization", "organisation", "company", "org"]),
        find(&["title", "job title"]),
        find(&["phone", "phones", "tel", "mobile"]),
        find(&["email", "emails", "e-mail"]),
        find(&["address"]),
        find(&["birthday", "bday"]),
        find(&["url", "website"]),
        find(&["note", "notes"]),
    ];
    let mut contacts = Vec::new();
    for row in r.records() {
        let row = row.map_err(|e| e.to_string())?;
        let cell = |i: Option<usize>| i.and_then(|i| row.get(i)).unwrap_or("").trim().to_string();
        let [org, title, phone, email, address, birthday, url, note] = columns;
        contacts.push(Contact {
            name: cell(Some(name)),
            organization: cell(org),
            title: cell(title),
            phones: list(&cell(phone)),
            emails: list(&cell(email)),
            address: cell(address),
            birthday: cell(birthday),
            url: cell(url),
            note: cell(note),
        });
    }
    Ok(contacts)
}

fn escape(value: &str) -> String {
    value
        .replace('\\', r"\\")
        .replace('\n', r"\n")
        .replace(',', r"\,")
        .replace(';', r"\;")
}

pub fn write(contacts: &[Contact]) -> String {
    let mut s = String::new();
    for c in contacts {
        s.push_str("BEGIN:VCARD\r\nVERSION:3.0\r\n");
        s.push_str(&format!("FN:{}\r\n", escape(&c.name)));
        s.push_str(&format!("N:{};;;;\r\n", escape(&c.name)));
        let fields = [
            ("ORG", &c.organization),
            ("TITLE", &c.title),
            ("BDAY", &c.birthday),
            ("URL", &c.url),
            ("NOTE", &c.note),
        ];
        for (key, value) in fields {
            if !value.is_empty() {
                s.push_str(&format!("{key}:{}\r\n", escape(value)));
            }
        }
        if !c.address.is_empty() {
            s.push_str(&format!("ADR:;;{};;;;\r\n", escape(&c.address)));
        }
        for phone in &c.phones {
            s.push_str(&format!("TEL:{}\r\n", escape(phone)));
        }
        for email in &c.emails {
            s.push_str(&format!("EMAIL:{}\r\n", escape(email)));
        }
        s.push_str("END:VCARD\r\n");
    }
    s
}
