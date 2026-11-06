//! RFC 822 messages.

use crate::{bad, html};
use deft_core::error::Result;
use mailparse::{MailHeaderMap, ParsedMail};

#[derive(Clone, Debug, Default)]
pub struct Attachment {
    pub name: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct Message {
    pub from: String,
    pub to: String,
    pub subject: String,
    pub date: String,
    pub body: String,
    pub attachments: Vec<Attachment>,
}

fn walk(
    part: &ParsedMail,
    plain: &mut Option<String>,
    rich: &mut Option<String>,
    out: &mut Vec<Attachment>,
) {
    if !part.subparts.is_empty() {
        for p in &part.subparts {
            walk(p, plain, rich, out);
        }
        return;
    }
    let disposition = part.get_content_disposition();
    let name = disposition
        .params
        .get("filename")
        .or_else(|| part.ctype.params.get("name"))
        .cloned();
    let is_attachment = matches!(
        disposition.disposition,
        mailparse::DispositionType::Attachment
    ) || name.is_some();
    if is_attachment {
        if let Ok(bytes) = part.get_body_raw() {
            let name = name.unwrap_or_else(|| format!("attachment-{}", out.len() + 1));
            out.push(Attachment { name, bytes });
        }
        return;
    }
    let Ok(body) = part.get_body() else {
        return;
    };
    match part.ctype.mimetype.as_str() {
        "text/plain" if plain.is_none() => *plain = Some(body),
        "text/html" if rich.is_none() => *rich = Some(body),
        _ => {}
    }
}

pub fn read(bytes: &[u8]) -> Result<Message> {
    let mail = mailparse::parse_mail(bytes).map_err(|e| bad("email", e))?;
    let get = |k: &str| mail.headers.get_first_value(k).unwrap_or_default();
    let (mut plain, mut rich, mut attachments) = (None, None, Vec::new());
    walk(&mail, &mut plain, &mut rich, &mut attachments);
    let body = match (plain, rich) {
        (Some(p), _) => p,
        (None, Some(h)) => html::to_text(&h)?,
        _ => String::new(),
    };
    Ok(Message {
        from: get("From"),
        to: get("To"),
        subject: get("Subject"),
        date: get("Date"),
        body,
        attachments,
    })
}
