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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anchors_follow_github() {
        let mut a = Anchors::default();
        assert_eq!(a.next("Table of Contents"), "table-of-contents");
        assert_eq!(a.next("`md toc` & more!"), "md-toc--more");
        assert_eq!(a.next("Table of Contents"), "table-of-contents-1");
        assert_eq!(a.next("😀 Smileys"), "-smileys");
    }
}
