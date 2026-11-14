//! Rich Text Format: paragraphs, headings, lists, tables, notes, links and
//! pictures. Bytes are decoded with the code page of the current font.

use crate::{Block, Document, Image, Span};
use encoding_rs::Encoding;
use std::collections::HashMap;

/// Where the text of a group goes.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Dest {
    Text,
    Skip,
    FontTable,
    StyleSheet,
    FieldInstruction,
    ListText,
    Picture,
    Title,
    Author,
}

#[derive(Clone, Copy)]
struct State {
    dest: Dest,
    bold: bool,
    italic: bool,
    strike: bool,
    /// Index into `Reader::links`, for the result of a HYPERLINK field.
    link: Option<usize>,
    font: i32,
    /// Bytes standing for one `\u` character.
    uc: usize,
    note: bool,
}

/// Groups that hold no document text.
const SKIPPED: &[&str] = &[
    "colortbl",
    "info",
    "header",
    "footer",
    "headerl",
    "headerr",
    "headerf",
    "footerl",
    "footerr",
    "footerf",
    "generator",
    "themedata",
    "colorschememapping",
    "latentstyles",
    "datastore",
    "object",
    "listtable",
    "listoverridetable",
    "rsidtbl",
    "xmlnstbl",
    "nonshppict",
    "bkmkstart",
    "bkmkend",
    "annotation",
    "atnid",
    "atnauthor",
    "pgdsctbl",
    "revtbl",
    "panose",
    "falt",
    "fname",
    "mmathPr",
    "wgrffmtfilter",
    "xe",
    "tc",
];

/// Starred groups that do hold text deft reads.
const KNOWN_STARRED: &[&str] = &["shppict", "fldinst", "ud", "listtext", "pntext"];

/// The code page of a font's character set.
fn charset_codepage(charset: i32) -> Option<u32> {
    Some(match charset {
        77 => 10000,
        128 => 932,
        129 | 130 => 949,
        134 => 936,
        136 => 950,
        161 => 1253,
        162 => 1254,
        163 => 1258,
        177 => 1255,
        178 => 1256,
        186 => 1257,
        204 => 1251,
        222 => 874,
        238 => 1250,
        _ => return None,
    })
}

fn encoding(codepage: u32) -> &'static Encoding {
    let label = match codepage {
        932 => "shift_jis".to_string(),
        936 => "gbk".to_string(),
        949 => "euc-kr".to_string(),
        950 => "big5".to_string(),
        10000 => "macintosh".to_string(),
        874 => "windows-874".to_string(),
        n @ 1250..=1258 => format!("windows-{n}"),
        _ => "windows-1252".to_string(),
    };
    Encoding::for_label(label.as_bytes()).unwrap_or(encoding_rs::WINDOWS_1252)
}

#[derive(Default)]
struct Paragraph {
    spans: Vec<Span>,
    in_table: bool,
    /// The list marker, such as "1." or a bullet.
    list: Option<String>,
    /// Left indent in twips, which says how deep a list item sits.
    indent: i32,
    style: i32,
    outline: Option<u8>,
}

struct Reader {
    ansi: u32,
    default_font: i32,
    /// Font number -> code page.
    fonts: HashMap<i32, u32>,
    font_def: i32,
    styles: HashMap<i32, String>,
    style_def: Option<i32>,
    /// Bytes not yet decoded, with the code page they are in.
    pending: Vec<u8>,
    pending_cp: u32,
    skip_chars: usize,
    high_half: Option<u32>,
    para: Paragraph,
    blocks: Vec<Block>,
    table: Vec<Vec<Vec<Span>>>,
    row: Vec<Vec<Span>>,
    cell: Vec<Span>,
    /// Footnote text, as it is read, and finished notes.
    note: Vec<Span>,
    notes: Vec<Vec<Span>>,
    instruction: String,
    links: Vec<Option<String>>,
    list_text: String,
    indents: Vec<i32>,
    /// The destination a `\upr` group interrupted, for its `\ud` part.
    upr: Dest,
    picture: String,
    picture_kind: &'static str,
    images: Vec<Image>,
    skipped_pictures: usize,
    title: String,
    author: String,
    style_name: String,
}

impl Reader {
    fn codepage(&self, s: &State) -> u32 {
        self.fonts.get(&s.font).copied().unwrap_or(self.ansi)
    }

    fn byte(&mut self, b: u8, s: &State) {
        if self.skip_chars > 0 {
            self.skip_chars -= 1;
            return;
        }
        let cp = self.codepage(s);
        if !self.pending.is_empty() && cp != self.pending_cp {
            self.flush(s);
        }
        self.pending_cp = cp;
        self.pending.push(b);
    }

    fn flush(&mut self, s: &State) {
        if self.pending.is_empty() {
            return;
        }
        let bytes = std::mem::take(&mut self.pending);
        let (text, _) = encoding(self.pending_cp).decode_without_bom_handling(&bytes);
        self.text(&text, s);
    }

    /// Decoded text, sent where the group's destination says.
    fn text(&mut self, text: &str, s: &State) {
        match s.dest {
            Dest::Text => {
                let span = Span {
                    text: text.to_string(),
                    bold: s.bold,
                    italic: s.italic,
                    strike: s.strike,
                    link: s.link.and_then(|i| self.links[i].clone()),
                    ..Default::default()
                };
                let target = if s.note {
                    &mut self.note
                } else {
                    &mut self.para.spans
                };
                match target.last_mut() {
                    Some(last)
                        if (last.bold, last.italic, last.strike, &last.link)
                            == (span.bold, span.italic, span.strike, &span.link)
                            && last.image.is_none()
                            && !last.note =>
                    {
                        last.text.push_str(&span.text)
                    }
                    _ => target.push(span),
                }
            }
            Dest::StyleSheet => self.style_name.push_str(text),
            Dest::FieldInstruction => self.instruction.push_str(text),
            Dest::ListText => self.list_text.push_str(text),
            Dest::Picture => self.picture.push_str(text),
            Dest::Title => self.title.push_str(text),
            Dest::Author => self.author.push_str(text),
            Dest::Skip | Dest::FontTable => {}
        }
    }

    fn char(&mut self, c: char, s: &State) {
        self.flush(s);
        let mut buf = [0; 4];
        self.text(c.encode_utf8(&mut buf), s);
    }

    /// End a paragraph: into the table cell, the list, or the body.
    fn paragraph(&mut self, s: &State) {
        self.flush(s);
        if s.note {
            self.note.push(Span::plain(" "));
            return;
        }
        let para = std::mem::take(&mut self.para);
        let keep = Paragraph {
            in_table: para.in_table,
            list: None,
            indent: para.indent,
            style: para.style,
            outline: para.outline,
            spans: vec![],
        };
        self.para = keep;
        if para.in_table {
            if !self.cell.is_empty() && !para.spans.is_empty() {
                self.cell.push(Span::plain(" "));
            }
            self.cell.extend(para.spans);
            return;
        }
        self.end_table();
        let mut spans = para.spans;
        crate::trim_spans(&mut spans);
        if spans.is_empty() {
            return;
        }
        let heading = para.outline.or_else(|| {
            let name = self.styles.get(&para.style)?.to_lowercase();
            if name == "title" {
                return Some(1);
            }
            name.strip_prefix("heading ")?.trim().parse().ok()
        });
        if para.list.is_none() {
            self.indents.clear();
        }
        if let Some(marker) = para.list {
            // Writers nest lists by indent as often as by level.
            while self.indents.last().is_some_and(|i| *i > para.indent) {
                self.indents.pop();
            }
            if self.indents.last() != Some(&para.indent) {
                self.indents.push(para.indent);
            }
            let level = self.indents.len() - 1;
            let marker = marker.trim();
            let ordered = marker.chars().next().is_some_and(char::is_alphanumeric)
                && marker.ends_with(['.', ')']);
            // The number the writer printed, when it is a plain one.
            let number = marker
                .trim_end_matches(['.', ')'])
                .parse::<u64>()
                .ok()
                .filter(|_| ordered);
            crate::docx::add_item(
                &mut self.blocks,
                level,
                ordered,
                number,
                vec![Block::Paragraph(spans)],
            );
        } else if let Some(level) = heading.filter(|l| (1..=6).contains(l)) {
            self.blocks.push(Block::Heading(level, spans));
        } else {
            self.blocks.push(Block::Paragraph(spans));
        }
    }

    fn end_cell(&mut self, s: &State) {
        self.para.in_table = true;
        self.paragraph(s);
        let mut cell = std::mem::take(&mut self.cell);
        crate::trim_spans(&mut cell);
        self.row.push(cell);
    }

    fn end_row(&mut self) {
        let row = std::mem::take(&mut self.row);
        if !row.is_empty() {
            self.table.push(row);
        }
    }

    fn end_table(&mut self) {
        self.end_row();
        if !self.table.is_empty() {
            let rows = std::mem::take(&mut self.table);
            self.blocks.push(Block::Table(rows, vec![]));
        }
    }

    fn finish_picture(&mut self) {
        let hex: Vec<u8> = self.picture.bytes().filter(u8::is_ascii_hexdigit).collect();
        self.picture.clear();
        let bytes: Vec<u8> = hex
            .chunks_exact(2)
            .filter_map(|p| u8::from_str_radix(std::str::from_utf8(p).ok()?, 16).ok())
            .collect();
        if bytes.is_empty() {
            return;
        }
        if self.picture_kind == "bin" {
            self.skipped_pictures += 1;
            return;
        }
        let name = format!("image{}.{}", self.images.len() + 1, self.picture_kind);
        self.images.push(Image {
            name: name.clone(),
            bytes,
        });
        self.para.spans.push(Span {
            image: Some(name),
            ..Default::default()
        });
    }

    /// The link a field's result gets: the address of a HYPERLINK field.
    fn field_link(&mut self) -> usize {
        let inst = std::mem::take(&mut self.instruction);
        let words: Vec<&str> = inst.split_whitespace().collect();
        let url = match words.as_slice() {
            ["HYPERLINK", rest @ ..] if !rest.contains(&"\\l") => rest
                .iter()
                .find(|w| !w.starts_with('\\'))
                .map(|w| w.trim_matches('"').to_string()),
            _ => None,
        };
        self.links.push(url.filter(|u| !u.is_empty()));
        self.links.len() - 1
    }
}

pub fn read(bytes: &[u8]) -> Document {
    let mut r = Reader {
        ansi: 1252,
        default_font: 0,
        fonts: HashMap::new(),
        font_def: 0,
        styles: HashMap::new(),
        style_def: None,
        pending: vec![],
        pending_cp: 1252,
        skip_chars: 0,
        high_half: None,
        para: Paragraph::default(),
        blocks: vec![],
        table: vec![],
        row: vec![],
        cell: vec![],
        note: vec![],
        notes: vec![],
        instruction: String::new(),
        links: vec![],
        list_text: String::new(),
        indents: vec![],
        upr: Dest::Text,
        picture: String::new(),
        picture_kind: "bin",
        images: vec![],
        skipped_pictures: 0,
        title: String::new(),
        author: String::new(),
        style_name: String::new(),
    };
    let mut stack = vec![State {
        dest: Dest::Text,
        bold: false,
        italic: false,
        strike: false,
        link: None,
        font: 0,
        uc: 1,
        note: false,
    }];
    let mut i = 0;
    while i < bytes.len() {
        let state = *stack.last().expect("one state");
        match bytes[i] {
            b'{' => {
                r.flush(&state);
                stack.push(state);
                i += 1;
                // {\*\destination ...} may be ignored unless deft knows it.
                if bytes[i..].starts_with(b"\\*") {
                    let rest = &bytes[i + 2..];
                    let rest = rest.strip_prefix(b"\\").unwrap_or(rest);
                    let word: Vec<u8> = rest
                        .iter()
                        .take_while(|b| b.is_ascii_alphabetic())
                        .copied()
                        .collect();
                    let known = KNOWN_STARRED.iter().any(|k| k.as_bytes() == word);
                    if !known {
                        stack.last_mut().expect("pushed").dest = Dest::Skip;
                    }
                }
            }
            b'}' => {
                r.flush(&state);
                i += 1;
                if stack.len() == 1 {
                    continue;
                }
                let closed = stack.pop().expect("more than one");
                let parent = *stack.last().expect("one state");
                r.skip_chars = 0;
                if closed.dest == Dest::ListText && parent.dest != Dest::ListText {
                    r.para.list = Some(std::mem::take(&mut r.list_text));
                }
                if closed.dest == Dest::Picture && parent.dest != Dest::Picture {
                    r.finish_picture();
                }
                if closed.dest == Dest::StyleSheet && parent.dest == Dest::StyleSheet {
                    r.style_def = None;
                }
                if closed.note && !parent.note {
                    let mut note = std::mem::take(&mut r.note);
                    crate::trim_spans(&mut note);
                    r.notes.push(note);
                }
            }
            b'\\' => {
                i += 1;
                let Some(&next) = bytes.get(i) else { break };
                if next.is_ascii_alphabetic() {
                    let start = i;
                    while bytes.get(i).is_some_and(u8::is_ascii_alphabetic) {
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
                    if word == "bin" {
                        // Raw picture bytes follow; store them as hex.
                        let n = value.unwrap_or(0).max(0) as usize;
                        let end = (i + n).min(bytes.len());
                        if state.dest == Dest::Picture {
                            for b in &bytes[i..end] {
                                r.picture.push_str(&format!("{b:02x}"));
                            }
                        }
                        i = end;
                        continue;
                    }
                    if word != "u" {
                        r.flush(&state);
                    }
                    control(&mut r, &mut stack, word, value);
                } else if next == b'\'' {
                    let hex = bytes.get(i + 1..i + 3).unwrap_or(&[]);
                    i += 3;
                    if let Some(v) = std::str::from_utf8(hex)
                        .ok()
                        .and_then(|h| u8::from_str_radix(h, 16).ok())
                    {
                        r.byte(v, &state);
                    }
                } else {
                    i += 1;
                    match next {
                        b'\\' | b'{' | b'}' => r.byte(next, &state),
                        b'~' => r.char('\u{a0}', &state),
                        b'_' => r.char('\u{2011}', &state),
                        b'\n' | b'\r' if state.dest == Dest::Text => r.paragraph(&state),
                        _ => {}
                    }
                }
            }
            b'\r' | b'\n' => i += 1,
            b => {
                i += 1;
                if state.dest == Dest::StyleSheet && b == b';' {
                    r.flush(&state);
                    if let Some(n) = r.style_def.take() {
                        let name = std::mem::take(&mut r.style_name);
                        r.styles.insert(n, name.trim().to_string());
                    }
                    continue;
                }
                r.byte(b, &state);
            }
        }
    }
    let last = *stack.last().expect("one state");
    r.paragraph(&State {
        note: false,
        ..last
    });
    r.end_table();
    let mut blocks = std::mem::take(&mut r.blocks);
    if !r.notes.is_empty() {
        blocks.push(Block::Rule);
        for (n, note) in r.notes.iter().enumerate() {
            let mut spans = vec![Span::note(format!("[{}]:", n + 1)), Span::plain(" ")];
            spans.extend(note.iter().cloned());
            blocks.push(Block::Paragraph(spans));
        }
    }
    let clean = |s: &str| Some(s.trim().to_string()).filter(|t| !t.is_empty());
    Document {
        title: clean(&r.title),
        author: clean(&r.author),
        blocks,
        images: r.images,
        warnings: if r.skipped_pictures > 0 {
            vec![format!(
                "{} pictures in a format deft cannot read were left out",
                r.skipped_pictures
            )]
        } else {
            vec![]
        },
        ..Default::default()
    }
}

fn control(r: &mut Reader, stack: &mut [State], word: &str, value: Option<i32>) {
    let state = *stack.last().expect("one state");
    let top = stack.last_mut().expect("one state");
    let on = value != Some(0);
    let text = top.dest == Dest::Text;
    match word {
        "ansicpg" => r.ansi = value.unwrap_or(1252).max(0) as u32,
        "deff" => {
            r.default_font = value.unwrap_or(0);
            top.font = r.default_font;
        }
        "fonttbl" => top.dest = Dest::FontTable,
        "f" if top.dest == Dest::FontTable => r.font_def = value.unwrap_or(0),
        "f" => top.font = value.unwrap_or(0),
        "fcharset" if top.dest == Dest::FontTable => {
            if let Some(cp) = value.and_then(charset_codepage) {
                r.fonts.insert(r.font_def, cp);
            }
        }
        "cpg" if top.dest == Dest::FontTable => {
            if let Some(cp) = value.filter(|v| *v > 0) {
                r.fonts.insert(r.font_def, cp as u32);
            }
        }
        "stylesheet" => top.dest = Dest::StyleSheet,
        "s" if top.dest == Dest::StyleSheet => {
            r.style_def = value;
            r.style_name.clear();
        }
        "cs" | "ts" | "ds" if top.dest == Dest::StyleSheet => r.style_def = None,
        "s" if text => r.para.style = value.unwrap_or(0),
        "outlinelevel" if text => {
            r.para.outline = value.and_then(|v| u8::try_from(v + 1).ok());
        }
        "b" => top.bold = on,
        "i" => top.italic = on,
        "strike" | "striked" => top.strike = on,
        "plain" => {
            top.bold = false;
            top.italic = false;
            top.strike = false;
            top.font = r.default_font;
        }
        "uc" => top.uc = value.unwrap_or(1).max(0) as usize,
        "pard" if text && !state.note => {
            // A list marker comes just before the \pard of its paragraph.
            r.para.in_table = false;
            r.para.indent = 0;
            r.para.style = 0;
            r.para.outline = None;
        }
        "intbl" if text && !state.note => r.para.in_table = true,
        "li" | "lin" if text => r.para.indent = value.unwrap_or(0),
        "listtext" | "pntext" if text => {
            top.dest = Dest::ListText;
            r.list_text.clear();
        }
        "par" | "sect" if text => r.paragraph(&state),
        "page" if text && !state.note => {
            r.paragraph(&state);
            r.end_table();
            r.blocks.push(Block::PageBreak);
        }
        "cell" if text => r.end_cell(&state),
        "nestcell" if text => r.char(' ', &state),
        "row" if text => {
            if !r.cell.is_empty() || !r.para.spans.is_empty() {
                r.end_cell(&state);
            }
            r.end_row();
        }
        "line" if text => r.char('\n', &state),
        "tab" if top.dest == Dest::ListText => {}
        "tab" => r.char('\t', &state),
        "emdash" => r.char('—', &state),
        "endash" => r.char('–', &state),
        "bullet" => r.char('•', &state),
        "lquote" => r.char('‘', &state),
        "rquote" => r.char('’', &state),
        "ldblquote" => r.char('“', &state),
        "rdblquote" => r.char('”', &state),
        "u" => {
            let v = value.unwrap_or(0);
            let code = if v < 0 { v + 65536 } else { v } as u32;
            r.flush(&state);
            r.skip_chars = state.uc;
            // Beyond U+FFFF, RTF writes two halves of a pair.
            if (0xD800..0xDC00).contains(&code) {
                r.high_half = Some(code);
                return;
            }
            let joined = match r.high_half.take() {
                Some(high) if (0xDC00..0xE000).contains(&code) => {
                    0x10000 + ((high - 0xD800) << 10) + (code - 0xDC00)
                }
                _ => code,
            };
            let saved = r.skip_chars;
            r.skip_chars = 0;
            r.char(char::from_u32(joined).unwrap_or('\u{FFFD}'), &state);
            r.skip_chars = saved;
        }
        "upr" => {
            r.upr = top.dest;
            top.dest = Dest::Skip;
        }
        "ud" => {
            // The Unicode part replaces what the ANSI part said.
            top.dest = r.upr;
            r.title.clear();
            r.author.clear();
        }
        "footnote" if text => {
            r.para
                .spans
                .push(Span::note(format!("[{}]", r.notes.len() + 1)));
            top.note = true;
        }
        "chftn" => {}
        "fldinst" if top.dest != Dest::Skip => {
            top.dest = Dest::FieldInstruction;
            r.instruction.clear();
        }
        "fldrslt" => top.link = Some(r.field_link()),
        "pict" if text => {
            top.dest = Dest::Picture;
            r.picture.clear();
            r.picture_kind = "bin";
        }
        "pngblip" => r.picture_kind = "png",
        "jpegblip" => r.picture_kind = "jpg",
        "emfblip" => r.picture_kind = "emf",
        "wmetafile" => r.picture_kind = "wmf",
        "macpict" => r.picture_kind = "pict",
        "dibitmap" | "wbitmap" => r.picture_kind = "bmp",
        "title" => top.dest = Dest::Title,
        "author" => top.dest = Dest::Author,
        w if SKIPPED.contains(&w) => top.dest = Dest::Skip,
        _ => {}
    }
}
