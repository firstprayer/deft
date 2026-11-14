//! Text rules shared by the document formats.

use std::collections::HashMap;

/// GitHub's anchor for a heading: lower case, spaces to hyphens, other
/// punctuation dropped.
pub fn anchor(text: &str) -> String {
    text.trim()
        .to_lowercase()
        .chars()
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            _ => None,
        })
        .collect()
}

/// Anchors for the headings of one document, in order. A repeated heading
/// gets `-1`, `-2` and so on, as on GitHub.
#[derive(Default)]
pub struct Anchors {
    seen: HashMap<String, usize>,
}

impl Anchors {
    pub fn next(&mut self, heading: &str) -> String {
        let base = anchor(heading);
        let count = self.seen.entry(base.clone()).or_default();
        let out = if *count == 0 {
            base
        } else {
            format!("{base}-{count}")
        };
        *count += 1;
        out
    }
}

/// Words as Word counts them: runs between spaces, except that each Chinese
/// character and each Japanese kana is a word of its own, since those
/// scripts do not put spaces between words.
pub fn count_words(text: &str) -> usize {
    let mut count = 0;
    let mut in_word = false;
    for c in text.chars() {
        let ideograph = matches!(c as u32,
            0x3040..=0x30FF | 0x31F0..=0x31FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF | 0xFF66..=0xFF9F | 0x20000..=0x3FFFF);
        // Ideographic punctuation and spaces separate, like spaces do.
        let separator = c.is_whitespace()
            || matches!(c as u32, 0x3000..=0x303F | 0xFF01..=0xFF0F | 0xFF1A..=0xFF20);
        if ideograph {
            count += 1;
            in_word = false;
        } else if separator {
            in_word = false;
        } else if !in_word {
            count += 1;
            in_word = true;
        }
    }
    count
}

/// The `<img ...>` tags in raw HTML, as byte ranges.
pub fn img_tags(html: &str) -> Vec<std::ops::Range<usize>> {
    // ASCII lowercasing keeps every byte where it was.
    let lower = html.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(at) = lower[from..].find("<img") {
        let start = from + at;
        let end = lower[start..]
            .find('>')
            .map_or(lower.len(), |e| start + e + 1);
        if lower[start + 4..]
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_whitespace() || c == '/' || c == '>')
        {
            out.push(start..end);
        }
        from = end.max(start + 4);
    }
    out
}

/// Where the value of attribute `name` sits in one HTML tag, as a byte range.
pub fn attribute(tag: &str, name: &str) -> Option<std::ops::Range<usize>> {
    let lower = tag.to_ascii_lowercase();
    let b = lower.as_bytes();
    let end = lower.len();
    let mut i = 0;
    while let Some(found) = lower[i..].find(name) {
        let s = i + found;
        i = s + name.len();
        // "src", not "data-src" or "srcset".
        if s == 0 || b[s - 1].is_ascii_alphanumeric() || matches!(b[s - 1], b'-' | b'_' | b':') {
            continue;
        }
        let mut j = s + name.len();
        while j < end && b[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= end || b[j] != b'=' {
            continue;
        }
        j += 1;
        while j < end && b[j].is_ascii_whitespace() {
            j += 1;
        }
        if j >= end {
            return None;
        }
        return Some(if matches!(b[j], b'"' | b'\'') {
            let quote = b[j] as char;
            let open = j + 1;
            open..lower[open..].find(quote).map_or(end, |e| open + e)
        } else {
            j..lower[j..]
                .find(|c: char| c.is_ascii_whitespace() || c == '>')
                .map_or(end, |e| j + e)
        });
    }
    None
}

/// Text of an HTML attribute value with the common entities read.
pub fn unescape_attribute(value: &str) -> String {
    value
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chinese_and_japanese_count_by_character() {
        assert_eq!(count_words("The quick brown fox"), 4);
        assert_eq!(count_words("西遊記，第一回"), 6);
        assert_eq!(count_words("吾輩は猫である。名前はまだ無い。"), 14);
        assert_eq!(count_words("Rust 1.83 是一种语言"), 7);
        assert_eq!(count_words("한국어 문장 입니다"), 3);
    }

    #[test]
    fn anchors_follow_github() {
        let mut a = Anchors::default();
        assert_eq!(a.next("Table of Contents"), "table-of-contents");
        assert_eq!(a.next("`md toc` & more!"), "md-toc--more");
        assert_eq!(a.next("Table of Contents"), "table-of-contents-1");
        assert_eq!(a.next("😀 Smileys"), "-smileys");
    }

    #[test]
    fn img_tags_and_their_attributes() {
        let html = "<p><IMG alt='A &amp; B' data-src=\"x\" src=\"img/a b.png\"/> <imgx src=no></p>";
        let tags = img_tags(html);
        assert_eq!(tags.len(), 1);
        let tag = &html[tags[0].clone()];
        assert_eq!(&tag[attribute(tag, "src").unwrap()], "img/a b.png");
        assert_eq!(
            unescape_attribute(&tag[attribute(tag, "alt").unwrap()]),
            "A & B"
        );
    }
}
