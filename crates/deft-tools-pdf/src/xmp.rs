//! The XMP metadata packet: reading its document fields and changing them in
//! place, so the rest of the packet (PDF/A identification, custom fields)
//! stays.

const DC: &str = "http://purl.org/dc/elements/1.1/";
const PDF: &str = "http://ns.adobe.com/pdf/1.3/";
const XMP: &str = "http://ns.adobe.com/xap/1.0/";
const RDF: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#";

/// Which XMP property holds an Info dictionary key, and how.
#[derive(Clone, Copy)]
enum Kind {
    /// A language alternative: dc:title, dc:description.
    Alt,
    /// An ordered list: dc:creator.
    Seq,
    /// Plain text.
    Text,
}

fn property(key: &[u8]) -> Option<(&'static str, &'static str, &'static str, Kind)> {
    Some(match key {
        b"Title" => (DC, "dc", "title", Kind::Alt),
        b"Author" => (DC, "dc", "creator", Kind::Seq),
        b"Subject" => (DC, "dc", "description", Kind::Alt),
        b"Keywords" => (PDF, "pdf", "Keywords", Kind::Text),
        b"Producer" => (PDF, "pdf", "Producer", Kind::Text),
        b"Creator" => (XMP, "xmp", "CreatorTool", Kind::Text),
        b"CreationDate" => (XMP, "xmp", "CreateDate", Kind::Text),
        b"ModDate" => (XMP, "xmp", "ModifyDate", Kind::Text),
        _ => return None,
    })
}

/// The value of the property for an Info dictionary key, if the packet has it.
pub(crate) fn read(packet: &str, key: &[u8]) -> Option<String> {
    let (ns, _, name, _) = property(key)?;
    let doc = roxmltree::Document::parse(packet).ok()?;
    // As an element, or as an attribute of an rdf:Description.
    let element = doc
        .descendants()
        .find(|n| n.tag_name().namespace() == Some(ns) && n.tag_name().name() == name);
    let text = match element {
        Some(e) => {
            let items: Vec<String> = e
                .descendants()
                .filter(|n| n.tag_name().namespace() == Some(RDF) && n.tag_name().name() == "li")
                .filter_map(|n| n.text().map(str::to_string))
                .collect();
            if items.is_empty() {
                e.text().unwrap_or("").to_string()
            } else {
                items.join("; ")
            }
        }
        None => doc
            .descendants()
            .find_map(|n| n.attribute((ns, name)))?
            .to_string(),
    };
    let text = text.trim().to_string();
    (!text.is_empty()).then_some(text)
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// Remove every `prefix:name` element and attribute from the packet text.
fn remove(packet: &str, prefix: &str, name: &str) -> String {
    let tag = format!("{prefix}:{name}");
    let mut out = packet.to_string();
    // Elements, written out or self-closing.
    let open = format!("<{tag}");
    let mut from = 0;
    while let Some(at) = out[from..].find(&open).map(|i| i + from) {
        let after = out[at + open.len()..].chars().next();
        if !matches!(after, Some('>' | '/' | ' ' | '\t' | '\r' | '\n')) {
            from = at + open.len();
            continue;
        }
        let Some(head_end) = out[at..].find('>').map(|i| at + i) else {
            break;
        };
        let end = if out[..head_end].ends_with('/') {
            head_end + 1
        } else {
            let close = format!("</{tag}>");
            match out[head_end..].find(&close) {
                Some(i) => head_end + i + close.len(),
                None => break,
            }
        };
        out.replace_range(at..end, "");
        from = at;
    }
    // Attributes: ` prefix:name="..."`.
    for quote in ['"', '\''] {
        let attr = format!("{tag}={quote}");
        while let Some(at) = out.find(&attr) {
            let start = out[..at].trim_end().len();
            let Some(len) = out[at + attr.len()..].find(quote) else {
                break;
            };
            out.replace_range(start..at + attr.len() + len + 1, "");
        }
    }
    out
}

/// The packet with these Info fields set. `None` when it is not a packet
/// this can change (no rdf:RDF).
pub(crate) fn update(packet: &str, fields: &[(&[u8], String)]) -> Option<String> {
    roxmltree::Document::parse(packet).ok()?;
    let mut out = packet.to_string();
    let mut added = String::new();
    for (key, value) in fields {
        let Some((_, prefix, name, kind)) = property(key) else {
            continue;
        };
        out = remove(&out, prefix, name);
        let v = escape(value);
        let element = match kind {
            Kind::Alt => format!(
                "<{prefix}:{name}><rdf:Alt><rdf:li xml:lang=\"x-default\">{v}</rdf:li></rdf:Alt></{prefix}:{name}>"
            ),
            Kind::Seq => format!(
                "<{prefix}:{name}><rdf:Seq><rdf:li>{v}</rdf:li></rdf:Seq></{prefix}:{name}>"
            ),
            Kind::Text => format!("<{prefix}:{name}>{v}</{prefix}:{name}>"),
        };
        added.push_str(&element);
    }
    if added.is_empty() {
        return Some(out);
    }
    let description = format!(
        "<rdf:Description rdf:about=\"\" xmlns:dc=\"{DC}\" xmlns:pdf=\"{PDF}\" xmlns:xmp=\"{XMP}\">{added}</rdf:Description>"
    );
    let close = out.rfind("</rdf:RDF>")?;
    out.insert_str(close, &description);
    // The result must still be XML.
    roxmltree::Document::parse(&out).ok()?;
    Some(out)
}

#[cfg(test)]
#[path = "xmp_tests.rs"]
mod tests;
