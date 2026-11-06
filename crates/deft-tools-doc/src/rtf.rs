//! Rich Text Format: paragraphs with bold and italic.

use crate::{Block, Document, Span};

#[derive(Clone, Copy, Default)]
struct State {
    bold: bool,
    italic: bool,
    skip: bool,
    unicode_skip: usize,
}

/// Destinations that hold no document text.
const IGNORED: &[&str] = &[
    "fonttbl",
    "colortbl",
    "stylesheet",
    "info",
    "pict",
    "header",
    "footer",
    "headerl",
    "headerr",
    "footerl",
    "footerr",
    "generator",
    "themedata",
    "colorschememapping",
    "latentstyles",
    "datastore",
    "object",
    "fldinst",
    "listtable",
    "listoverridetable",
    "rsidtbl",
    "xmlnstbl",
];

/// One Windows-1252 byte. Only 0x80..0xA0 differ from Latin-1.
fn windows_1252(b: u8) -> char {
    const HIGH: &str = "€\u{81}‚ƒ„…†‡ˆ‰Š‹Œ\u{8D}Ž\u{8F}\u{90}‘’“”•–—˜™š›œ\u{9D}žŸ";
    if (0x80..0xA0).contains(&b) {
        HIGH.chars().nth((b - 0x80) as usize).unwrap_or('?')
    } else {
        b as char
    }
}

fn is_rtf_letter(b: u8) -> bool {
    b.is_ascii_alphabetic()
}

pub fn read(bytes: &[u8]) -> Document {
    let mut stack = vec![State::default()];
    let mut paragraphs: Vec<Vec<Span>> = vec![vec![]];
    let mut i = 0;
    let mut pending_skip = 0usize;
    let push_char = |paragraphs: &mut Vec<Vec<Span>>, c: char, s: State| {
        let para = paragraphs.last_mut().expect("one paragraph");
        match para.last_mut() {
            Some(last) if last.bold == s.bold && last.italic == s.italic => last.text.push(c),
            _ => para.push(Span {
                text: c.to_string(),
                bold: s.bold,
                italic: s.italic,
                ..Default::default()
            }),
        }
    };
    while i < bytes.len() {
        let state = *stack.last().expect("one state");
        match bytes[i] {
            b'{' => {
                stack.push(state);
                i += 1;
                // {\*\destination ...} is optional and safe to ignore.
                if bytes[i..].starts_with(b"\\*") {
                    stack.last_mut().expect("pushed").skip = true;
                }
            }
            b'}' => {
                if stack.len() > 1 {
                    stack.pop();
                }
                i += 1;
            }
            b'\\' => {
                i += 1;
                let Some(&next) = bytes.get(i) else { break };
                if is_rtf_letter(next) {
                    let start = i;
                    while bytes.get(i).is_some_and(|b| is_rtf_letter(*b)) {
                        i += 1;
                    }
                    let word = std::str::from_utf8(&bytes[start..i]).unwrap_or("");
                    let num_start = i;
                    if bytes.get(i) == Some(&b'-') {
                        i += 1;
                    }
                    while bytes.get(i).is_some_and(u8::is_ascii_digit) {
                        i += 1;
                    }
                    let value: Option<i32> = std::str::from_utf8(&bytes[num_start..i])
                        .ok()
                        .and_then(|n| n.parse().ok());
                    if bytes.get(i) == Some(&b' ') {
                        i += 1;
                    }
                    let top = stack.last_mut().expect("one state");
                    match word {
                        "b" => top.bold = value != Some(0),
                        "i" => top.italic = value != Some(0),
                        "plain" => {
                            top.bold = false;
                            top.italic = false;
                        }
                        "uc" => top.unicode_skip = value.unwrap_or(1).max(0) as usize,
                        "par" | "sect" | "page" | "row" if !top.skip => paragraphs.push(vec![]),
                        "line" if !top.skip => push_char(&mut paragraphs, '\n', state),
                        "tab" | "cell" if !top.skip => push_char(&mut paragraphs, '\t', state),
                        "u" if !top.skip => {
                            let v = value.unwrap_or(0);
                            let code = if v < 0 { v + 65536 } else { v } as u32;
                            push_char(&mut paragraphs, char::from_u32(code).unwrap_or('?'), state);
                            pending_skip = state.unicode_skip.max(1);
                        }
                        w if IGNORED.contains(&w) => top.skip = true,
                        _ => {}
                    }
                } else if next == b'\'' {
                    let hex = bytes.get(i + 1..i + 3).unwrap_or(&[]);
                    let value = std::str::from_utf8(hex)
                        .ok()
                        .and_then(|h| u8::from_str_radix(h, 16).ok());
                    i += 3;
                    if pending_skip > 0 {
                        pending_skip -= 1;
                    } else if let Some(v) = value
                        && !state.skip
                    {
                        push_char(&mut paragraphs, windows_1252(v), state);
                    }
                } else {
                    if !state.skip {
                        match next {
                            b'\\' | b'{' | b'}' => push_char(&mut paragraphs, next as char, state),
                            b'~' => push_char(&mut paragraphs, '\u{a0}', state),
                            b'\n' | b'\r' => paragraphs.push(vec![]),
                            _ => {}
                        }
                    }
                    i += 1;
                }
            }
            b'\r' | b'\n' => i += 1,
            b => {
                i += 1;
                if pending_skip > 0 {
                    pending_skip -= 1;
                } else if !state.skip {
                    push_char(&mut paragraphs, windows_1252(b), state);
                }
            }
        }
    }
    let blocks = paragraphs
        .into_iter()
        .filter(|p| p.iter().any(|s| !s.text.trim().is_empty()))
        .map(Block::Paragraph)
        .collect();
    Document {
        blocks,
        ..Default::default()
    }
}
