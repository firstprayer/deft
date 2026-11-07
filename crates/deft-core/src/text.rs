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
}
