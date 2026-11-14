//! Text as a page shows it, read through the interpreter that renders the page.
//! That one reader knows every font, Form XObjects and annotation appearances,
//! and gives real positions, so text, search and tables all agree.

use crate::Renderer;
use crate::layout::Run;
use deft_core::error::Result;
use hayro::hayro_interpret::font::{Glyph as Drawn, GlyphRun};
use hayro::hayro_interpret::hayro_cmap::BfString;
use hayro::hayro_interpret::util::TransformExt;
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps,
    InterpreterCache, SoftMask, interpret_page,
};
use hayro::kurbo::{BezPath, Point, Rect, Vec2};
use lopdf::Document;
use std::collections::HashSet;

/// Glyph space: the interpreter scales every font to this many units per em.
const UNITS: f64 = 1000.0;
/// A gap this wide, in ems, is a space between words.
const WORD_GAP: f64 = 0.2;
/// On a line whose letters touch, a gap this wide is a space too: justified
/// text squeezes word spaces below `WORD_GAP`.
const TIGHT_WORD_GAP: f64 = 0.12;
/// Letters this close, in ems, touch.
const TOUCHING: f64 = 0.03;
/// One this wide separates cells: LaTeX writes a whole table row in one operator.
const CELL_GAP: f64 = 0.7;
/// A step off the baseline this tall, in ems, starts a new line. Less is a
/// superscript or a subscript.
const LINE_STEP: f64 = 0.6;
/// How far a box reaches above and below the baseline, in ems.
const ASCENT: f64 = 0.95;
const DESCENT: f64 = 0.3;

/// One glyph as drawn, in points from the top-left of the page as shown.
#[derive(Clone, Debug)]
pub struct Glyph {
    pub text: String,
    /// Where it starts and ends on its baseline.
    pub start: Point,
    pub end: Point,
    /// One em up from the baseline.
    pub up: Vec2,
    /// The operator that drew it.
    pub run: usize,
}

impl Glyph {
    fn size(&self) -> f64 {
        self.up.hypot()
    }

    /// The way the text runs, as a unit vector. Down the page is positive y.
    fn direction(&self) -> Vec2 {
        Vec2::new(-self.up.y, self.up.x) / self.size().max(f64::EPSILON)
    }

    /// How far `next` starts past this glyph's end, and how far off its baseline.
    fn step_to(&self, next: &Glyph) -> (f64, f64) {
        let gap = next.start - self.end;
        let way = self.direction();
        (gap.dot(way), gap.cross(way).abs())
    }

    fn same_line(&self, next: &Glyph) -> bool {
        let (along, across) = self.step_to(next);
        let size = self.size().max(next.size());
        // Vertical writing: one operator sets its glyphs down a column.
        let step = next.start - self.start;
        let fall = -step.dot(self.up) / self.size().max(f64::EPSILON);
        let column = next.run == self.run
            && step.dot(self.direction()).abs() < size * 0.5
            && fall > 0.0
            && fall < size * 1.6;
        column || (across <= size * LINE_STEP && along >= -size * 2.0)
    }

    fn is_blank(&self) -> bool {
        self.text.trim().is_empty()
    }

    /// The upright box around the glyph, as (left, top, right, bottom).
    fn bounds(&self) -> [f64; 4] {
        let corners = [
            self.start + self.up * ASCENT,
            self.start - self.up * DESCENT,
            self.end + self.up * ASCENT,
            self.end - self.up * DESCENT,
        ];
        let xs = corners.map(|p| p.x);
        let ys = corners.map(|p| p.y);
        [
            xs.into_iter().fold(f64::MAX, f64::min),
            ys.into_iter().fold(f64::MAX, f64::min),
            xs.into_iter().fold(f64::MIN, f64::max),
            ys.into_iter().fold(f64::MIN, f64::max),
        ]
    }
}

/// What a glyph's Unicode value reads as: ligatures become letters, and control
/// characters go.
fn plain(text: &str) -> String {
    let mut out = String::new();
    for c in text.chars() {
        match c {
            'ﬀ' => out.push_str("ff"),
            'ﬁ' => out.push_str("fi"),
            'ﬂ' => out.push_str("fl"),
            'ﬃ' => out.push_str("ffi"),
            'ﬄ' => out.push_str("ffl"),
            'ﬅ' | 'ﬆ' => out.push_str("st"),
            '\u{A0}' | '\t' => out.push(' '),
            // Arabic presentation forms are the shapes of letters, not letters.
            '\u{FB50}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFC}' => {
                use unicode_normalization::UnicodeNormalization;
                out.extend(std::iter::once(c).nfkc());
            }
            // A glyph that is part of a cluster another glyph names.
            '\u{200B}' => {}
            c if c.is_control() => {}
            c => out.push(c),
        }
    }
    out
}

fn unicode(glyph: &Drawn<'_>) -> String {
    match glyph.as_unicode() {
        Some(BfString::Char(c)) => plain(c.encode_utf8(&mut [0; 4])),
        Some(BfString::String(s)) => plain(&s),
        None => match glyph {
            // Nothing to see, so it is a space with no name.
            Drawn::Outline(o) if o.outline().segments().next().is_none() => " ".to_string(),
            _ => "\u{FFFD}".to_string(),
        },
    }
}

/// Collects glyphs and ignores everything else that is drawn.
#[derive(Default)]
struct Collector {
    glyphs: Vec<Glyph>,
    runs: usize,
    /// Where images are drawn, as (left, top, right, bottom).
    images: Vec<[f64; 4]>,
}

impl Collector {
    /// Fill-and-stroke text arrives twice, once for each.
    fn repeats(&self, found: &[Glyph]) -> bool {
        let last = self.glyphs.len().checked_sub(found.len());
        last.is_some_and(|at| {
            self.glyphs[at..]
                .iter()
                .zip(found)
                .all(|(a, b)| a.text == b.text && (a.start - b.start).hypot() < 0.01)
        })
    }
}

impl<'a> Device<'a> for Collector {
    fn draw_path(&mut self, _: &BezPath, _: DrawProps<'a>, _: &DrawMode) {}
    fn push_clip_path(&mut self, _: &ClipPath) {}
    fn push_transparency_group(&mut self, _: f32, _: Option<SoftMask<'a>>, _: BlendMode) {}
    fn draw_image(&mut self, image: Image<'a, '_>, props: ImageDrawProps<'a>) {
        // The transform takes the image's pixels to the page.
        let (w, h) = (f64::from(image.width()), f64::from(image.height()));
        let corners = [(0.0, 0.0), (w, 0.0), (0.0, h), (w, h)]
            .map(|(x, y)| props.transform * Point::new(x, y));
        let xs = corners.map(|p| p.x);
        let ys = corners.map(|p| p.y);
        self.images.push([
            xs.into_iter().fold(f64::MAX, f64::min),
            ys.into_iter().fold(f64::MAX, f64::min),
            xs.into_iter().fold(f64::MIN, f64::max),
            ys.into_iter().fold(f64::MIN, f64::max),
        ]);
    }
    fn pop_clip(&mut self) {}
    fn pop_transparency_group(&mut self) {}

    fn draw_glyph_run(&mut self, run: &GlyphRun<'_, 'a>, props: DrawProps<'a>, _: &DrawMode) {
        let origin = |i: usize| (props.transform * run.glyphs()[i].transform()) * Point::ZERO;
        let mut found = Vec::with_capacity(run.glyphs().len());
        for (i, glyph) in run.glyphs().iter().enumerate() {
            let t = props.transform * glyph.transform();
            let start = t * Point::ZERO;
            let advance = match &**glyph {
                Drawn::Outline(o) => o.advance_width().map(f64::from),
                Drawn::Type3(_) => None,
            };
            let end = match advance {
                Some(width) => t * Point::new(width, 0.0),
                None if i + 1 < run.glyphs().len() => origin(i + 1),
                None => t * Point::new(UNITS / 2.0, 0.0),
            };
            found.push(Glyph {
                text: unicode(glyph),
                start,
                end,
                up: t * Point::new(0.0, UNITS) - start,
                run: self.runs,
            });
        }
        join_gbk(&mut found);
        if !self.repeats(&found) {
            self.glyphs.append(&mut found);
            self.runs += 1;
        }
    }
}

/// A GBK font's high bytes arrive as private-use characters, one glyph each:
/// each pair is one Chinese character, given to its first glyph.
fn join_gbk(glyphs: &mut [Glyph]) {
    let byte = |g: &Glyph| -> Option<u8> {
        let mut chars = g.text.chars();
        let c = chars.next()? as u32;
        let base = crate::fonts::GBK_BYTES;
        (chars.next().is_none() && (base..base + 0x80).contains(&c))
            .then(|| (c - base + 0x80) as u8)
    };
    let mut i = 0;
    while i < glyphs.len() {
        let Some(lead) = byte(&glyphs[i]) else {
            i += 1;
            continue;
        };
        let trail = glyphs.get(i + 1).and_then(|g| {
            let c = g.text.chars().next()? as u32;
            match byte(g) {
                Some(b) => Some(b),
                // A trail byte can be ASCII.
                None if g.text.chars().count() == 1 && (0x40..0x80).contains(&c) => Some(c as u8),
                None => None,
            }
        });
        let pair = trail.map(|t| {
            let bytes = [lead, t];
            let (text, bad) = encoding_rs::GBK.decode_without_bom_handling(&bytes);
            (text.into_owned(), bad)
        });
        match pair {
            Some((text, false)) => {
                glyphs[i].text = text;
                glyphs[i + 1].text = String::new();
                glyphs[i].end = glyphs[i + 1].end;
                i += 2;
                // These writers pad each character with a space byte, the
                // other half of its full-width cell.
                if glyphs.get(i).is_some_and(|g| g.text == " ") {
                    glyphs[i].text = String::new();
                    i += 1;
                }
            }
            _ => {
                glyphs[i].text = "\u{FFFD}".to_string();
                i += 1;
            }
        }
    }
}

/// Each character in drawing order with its glyph. A space or line break the
/// positions call for has no glyph.
fn reading(glyphs: &[Glyph]) -> Vec<(char, Option<usize>)> {
    let mut out: Vec<(char, Option<usize>)> = Vec::new();
    let tight = tight_lines(glyphs);
    // The last glyph with text: one whose text another glyph of its cluster
    // carries is passed over.
    let mut last: Option<&Glyph> = None;
    for (i, glyph) in glyphs.iter().enumerate() {
        if glyph.text.is_empty() {
            continue;
        }
        if let Some(last) = last {
            let word_gap = if tight[i] { TIGHT_WORD_GAP } else { WORD_GAP };
            let blank = out.last().is_none_or(|(c, _)| c.is_whitespace());
            // Chinese and Japanese put no spaces between words.
            let close = out.last().is_some_and(|(c, _)| is_cjk(*c))
                && glyph.text.chars().next().is_some_and(is_cjk);
            if !last.same_line(glyph) {
                out.push(('\n', None));
            } else if !blank && !glyph.is_blank() && !close {
                let (along, _) = last.step_to(glyph);
                if along > last.size().max(glyph.size()) * word_gap {
                    out.push((' ', None));
                }
            }
        }
        out.extend(glyph.text.chars().map(|c| (c, Some(i))));
        last = Some(glyph);
    }
    logical_order(&mut out, glyphs);
    despace(&mut out);
    out
}

/// For each glyph, whether the letters of its line touch: the middle gap
/// between them is under `TOUCHING`. Such a line is set tight, so a narrower
/// gap is still a space between words.
fn tight_lines(glyphs: &[Glyph]) -> Vec<bool> {
    let mut out = vec![false; glyphs.len()];
    let mut start = 0;
    while start < glyphs.len() {
        let mut end = start + 1;
        let mut gaps = Vec::new();
        let mut prev = start;
        while end < glyphs.len() && glyphs[prev].same_line(&glyphs[end]) {
            let (a, b) = (&glyphs[prev], &glyphs[end]);
            if !a.is_blank() && !b.is_blank() && !a.text.is_empty() && !b.text.is_empty() {
                gaps.push(a.step_to(b).0 / a.size().max(b.size()).max(f64::EPSILON));
            }
            prev = end;
            end += 1;
        }
        if gaps.len() >= 8 {
            gaps.sort_by(f64::total_cmp);
            let middle = gaps[gaps.len() / 2];
            if middle.abs() < TOUCHING {
                out[start..end].fill(true);
            }
        }
        start = end;
    }
    out
}

/// Text that has a space glyph after nearly every letter, and two between
/// words, as some OCR layers are written ("T h e  N a ti o n al"): drop the
/// single spaces and keep one of each pair. Each line is judged on its own.
fn despace(out: &mut Vec<(char, Option<usize>)>) {
    let mut kept = Vec::with_capacity(out.len());
    for line in out.split_inclusive(|c| c.0 == '\n') {
        if !letter_spaced(line) {
            kept.extend_from_slice(line);
            continue;
        }
        let mut i = 0;
        while i < line.len() {
            if line[i].0 == ' ' {
                let mut j = i;
                while j < line.len() && line[j].0 == ' ' {
                    j += 1;
                }
                if j - i >= 2 {
                    kept.push(line[i]);
                }
                i = j;
            } else {
                kept.push(line[i]);
                i += 1;
            }
        }
    }
    *out = kept;
}

/// Whether a line reads as letters with single spaces between them and
/// double spaces between words: mostly pieces of one or two letters, and at
/// least one double space. Chinese, Japanese and Korean do not count: their
/// words can be one or two characters long.
fn letter_spaced(line: &[(char, Option<usize>)]) -> bool {
    let text: String = line.iter().map(|c| c.0).collect();
    let text = text.trim();
    if !text.contains("  ") {
        return false;
    }
    let pieces: Vec<&str> = text.split(' ').filter(|p| !p.is_empty()).collect();
    let short = pieces
        .iter()
        .filter(|p| p.chars().count() <= 2 && p.chars().all(|c| c.is_alphabetic() && !is_cjk(c)))
        .count();
    pieces.len() >= 6 && short * 10 >= pieces.len() * 7
}

fn is_rtl(c: char) -> bool {
    matches!(c, '\u{0590}'..='\u{08FF}' | '\u{FB1D}'..='\u{FDFF}' | '\u{FE70}'..='\u{FEFF}')
        && !c.is_ascii_digit()
        && !('\u{0660}'..='\u{0669}').contains(&c)
}

/// Right-to-left letters drawn one glyph each, left to right as they show,
/// are read in the reverse order: a stretch of them, with the spaces and
/// punctuation between, is turned round glyph by glyph. Text whose glyphs
/// name whole words is already in reading order and stays.
fn logical_order(out: &mut [(char, Option<usize>)], glyphs: &[Glyph]) {
    // One letter, or a ligature of a few.
    let letter = |g: Option<usize>| {
        g.is_some_and(|i| {
            let t = &glyphs[i].text;
            t.chars().count() <= 4 && t.chars().all(is_rtl)
        })
    };
    let mut i = 0;
    while i < out.len() {
        if !(is_rtl(out[i].0) && letter(out[i].1)) {
            i += 1;
            continue;
        }
        let mut end = i + 1;
        let mut last = i;
        while end < out.len() && out[end].0 != '\n' {
            let (c, g) = out[end];
            if is_rtl(c) && letter(g) {
                last = end;
            } else if c.is_alphanumeric() || (g.is_some() && !c.is_whitespace() && !letter(g)) {
                break;
            }
            end += 1;
        }
        // Drawn right to left, the glyphs are in reading order already.
        let (Some(a), Some(b)) = (out[i].1, out[last].1) else {
            i = last + 1;
            continue;
        };
        let rightward = (glyphs[b].start - glyphs[a].start).dot(glyphs[a].direction()) > 0.0;
        if !rightward {
            i = last + 1;
            continue;
        }
        // Turn the glyphs round, each keeping its own characters in order.
        let mut groups: Vec<Vec<(char, Option<usize>)>> = Vec::new();
        for entry in &out[i..=last] {
            match groups.last_mut() {
                Some(group) if entry.1.is_some() && group[0].1 == entry.1 => group.push(*entry),
                _ => groups.push(vec![*entry]),
            }
        }
        let turned: Vec<(char, Option<usize>)> = groups.into_iter().rev().flatten().collect();
        out[i..=last].copy_from_slice(&turned);
        i = last + 1;
    }
}

/// Han, kana, Hangul and the full-width forms set among them.
fn is_cjk(c: char) -> bool {
    matches!(c,
        '\u{3000}'..='\u{30FF}' | '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}'
        | '\u{AC00}'..='\u{D7AF}' | '\u{F900}'..='\u{FAFF}' | '\u{FF00}'..='\u{FFEF}'
        | '\u{2026}')
}

/// Where a match starting at `from` ends. The line break after a hyphen in `wraps`
/// is passed over, and so is the hyphen itself when the word does not have one.
fn match_at(
    text: &[(char, usize)],
    from: usize,
    wanted: &[char],
    wraps: &HashSet<usize>,
) -> Option<usize> {
    let hop = |i: usize| i + usize::from(text.get(i).is_some_and(|c| c.0 == ' '));
    let (mut i, mut matched) = (from, 0);
    while matched < wanted.len() {
        let (c, at) = *text.get(i)?;
        let wrap = wraps.contains(&at);
        if c == wanted[matched] {
            matched += 1;
            i = if wrap && matched < wanted.len() {
                hop(i + 1)
            } else {
                i + 1
            };
        } else if wrap && matched > 0 {
            i = hop(i + 1);
        } else {
            return None;
        }
    }
    Some(i)
}

/// The text of one page.
pub struct PageText {
    pub glyphs: Vec<Glyph>,
    /// The size of the page as shown.
    pub width: f64,
    pub height: f64,
    /// Where images are drawn, as (left, top, right, bottom) from the top-left.
    pub images: Vec<[f64; 4]>,
}

/// A place the text was found.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    /// Characters, as an index into `PageText::flat`.
    pub start: usize,
    pub end: usize,
    /// One box per line it covers: x, y, width, height from the top-left.
    pub regions: Vec<[f64; 4]>,
}

impl PageText {
    /// How many characters the page shows that could be decoded, not counting
    /// white space.
    pub fn chars(&self) -> usize {
        self.glyphs
            .iter()
            .flat_map(|g| g.text.chars())
            .filter(|c| !c.is_whitespace() && *c != '\u{FFFD}')
            .count()
    }

    fn share(&self, boxes: impl Iterator<Item = [f64; 4]>) -> f64 {
        let area = (self.width * self.height).max(1.0);
        let inside: f64 = boxes
            .map(|[l, t, r, b]| {
                let w = r.min(self.width) - l.max(0.0);
                let h = b.min(self.height) - t.max(0.0);
                w.max(0.0) * h.max(0.0)
            })
            .sum();
        (inside / area).min(1.0)
    }

    /// The share of the page, from 0 to 1, that images cover.
    pub fn image_cover(&self) -> f64 {
        self.share(self.images.iter().copied())
    }

    /// The share of the page, from 0 to 1, that the boxes of its characters cover.
    pub fn text_cover(&self) -> f64 {
        self.share(
            self.glyphs
                .iter()
                .filter(|g| !g.is_blank())
                .map(Glyph::bounds),
        )
    }

    /// A page that is a picture with little or no text on it, such as a scan
    /// without a text layer or with only a short stamp: reading it with OCR
    /// finds the text.
    pub fn is_scan(&self) -> bool {
        self.image_cover() > 0.5 && (self.chars() < 20 || self.text_cover() < 0.02)
    }

    /// The page as lines, in drawing order.
    pub fn text(&self) -> String {
        let all: String = reading(&self.glyphs).iter().map(|c| c.0).collect();
        let lines: Vec<&str> = all.lines().map(str::trim_end).collect();
        lines.join("\n").trim_end().to_string()
    }

    /// The characters of the page with every stretch of white space as one space,
    /// and where a hyphen ends a line.
    fn flat_chars(&self) -> (Vec<(char, Option<usize>)>, HashSet<usize>) {
        let mut out: Vec<(char, Option<usize>)> = Vec::new();
        let mut wraps = HashSet::new();
        for (c, glyph) in reading(&self.glyphs) {
            if c == '\n'
                && let Some(last) = out.iter().rposition(|(p, _)| *p != ' ')
                && matches!(out[last].0, '-' | '\u{AD}' | '\u{2010}')
            {
                wraps.insert(last);
            }
            if !c.is_whitespace() {
                out.push((c, glyph));
            } else if out.last().is_some_and(|(p, _)| *p != ' ') {
                out.push((' ', None));
            }
        }
        (out, wraps)
    }

    /// The page as one line, as `Hit` counts it.
    pub fn flat(&self) -> Vec<char> {
        self.flat_chars().0.iter().map(|c| c.0).collect()
    }

    /// Every place `needle` appears, ignoring case and line breaks. A word split
    /// by a hyphen at the end of a line counts as whole.
    pub fn find(&self, needle: &str) -> Vec<Hit> {
        let wanted: Vec<char> = plain(needle)
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .flat_map(char::to_lowercase)
            .collect();
        if wanted.is_empty() {
            return vec![];
        }
        let (flat, wraps) = self.flat_chars();
        // Lower case can be longer than the original, so keep the way back.
        let lower: Vec<(char, usize)> = flat
            .iter()
            .enumerate()
            .flat_map(|(i, (c, _))| c.to_lowercase().map(move |l| (l, i)))
            .collect();
        let mut hits = Vec::new();
        let mut at = 0;
        while at < lower.len() {
            let Some(next) = match_at(&lower, at, &wanted, &wraps) else {
                at += 1;
                continue;
            };
            let (start, end) = (lower[at].1, lower[next - 1].1 + 1);
            hits.push(Hit {
                start,
                end,
                regions: self.regions(flat[start..end].iter().filter_map(|c| c.1)),
            });
            at = next;
        }
        hits
    }

    /// A box per line around these glyphs.
    fn regions(&self, glyphs: impl Iterator<Item = usize>) -> Vec<[f64; 4]> {
        let mut out: Vec<[f64; 4]> = Vec::new();
        let mut last: Option<usize> = None;
        for i in glyphs {
            if last == Some(i) {
                continue;
            }
            let b = self.glyphs[i].bounds();
            match (last, out.last_mut()) {
                (Some(p), Some(r)) if self.glyphs[p].same_line(&self.glyphs[i]) => {
                    *r = [
                        r[0].min(b[0]),
                        r[1].min(b[1]),
                        r[2].max(b[2]),
                        r[3].max(b[3]),
                    ];
                }
                _ => out.push(b),
            }
            last = Some(i);
        }
        out.iter()
            .map(|r| [r[0] - 1.0, r[1] - 1.0, r[2] - r[0] + 2.0, r[3] - r[1] + 2.0])
            .collect()
    }

    /// Pieces of text for finding tables, with the baseline measured from the
    /// bottom of the page. One piece is what one operator drew, cut at wide gaps.
    pub fn runs(&self) -> Vec<Run> {
        let apart = |a: &Glyph, b: &Glyph| {
            a.run != b.run || !a.same_line(b) || a.step_to(b).0 >= a.size() * CELL_GAP
        };
        let mut out = Vec::new();
        for piece in self.glyphs.chunk_by(|a, b| !apart(a, b)) {
            let first = piece.iter().position(|g| !g.is_blank());
            let last = piece.iter().rposition(|g| !g.is_blank());
            let (Some(first), Some(last)) = (first, last) else {
                continue;
            };
            let kept = &piece[first..=last];
            let text: String = reading(kept).iter().map(|c| c.0).collect();
            out.push(Run {
                x: kept[0].start.x,
                y: self.height - kept[0].start.y,
                size: kept[0].size(),
                text,
                width: (kept[kept.len() - 1].end - kept[0].start).hypot(),
            });
        }
        out
    }
}

impl Renderer {
    /// Read pages (zero-based) one at a time, sharing parsed fonts between them.
    pub fn each_page_text(&self, indexes: &[usize], mut read: impl FnMut(usize, PageText)) {
        let cache = InterpreterCache::new();
        let pages = self.pdf.pages();
        for &i in indexes {
            let Some(page) = pages.get(i) else {
                continue;
            };
            let (width, height) = page.render_dimensions();
            let mut context = Context::new(
                page.initial_transform(true).to_kurbo(),
                Rect::new(0.0, 0.0, width as f64, height as f64),
                &cache,
                page.xref(),
                crate::fonts::interpreter_settings(),
            );
            let mut found = Collector::default();
            interpret_page(page, &mut context, &mut found);
            read(
                i,
                PageText {
                    glyphs: found.glyphs,
                    width: width as f64,
                    height: height as f64,
                    images: found.images,
                },
            );
        }
    }

    /// The text of these pages, one string each.
    pub fn page_texts(&self, indexes: &[usize]) -> Vec<String> {
        let mut out = Vec::with_capacity(indexes.len());
        self.each_page_text(indexes, |_, page| out.push(page.text()));
        out
    }
}

/// A reader for a document in memory.
pub fn reader(doc: &Document) -> Result<Renderer> {
    Renderer::of(doc)
}

/// The share of `text` that could not be decoded, from 0 to 1.
pub fn undecoded(text: &str) -> f64 {
    let (mut bad, mut all) = (0usize, 0usize);
    for c in text.chars().filter(|c| !c.is_whitespace()) {
        all += 1;
        bad += usize::from(c == '\u{FFFD}');
    }
    bad as f64 / all.max(1) as f64
}

#[cfg(test)]
#[path = "text_tests.rs"]
mod tests;
