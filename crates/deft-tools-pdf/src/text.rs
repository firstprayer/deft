//! Text as a page shows it, read through the interpreter that renders the page.
//! That one reader knows every font, Form XObjects and annotation appearances,
//! and gives real positions, so text, search, tables and redaction all agree.

use crate::Renderer;
use crate::layout::Run;
use deft_core::error::Result;
use hayro::hayro_interpret::font::{Glyph as Drawn, GlyphRun};
use hayro::hayro_interpret::hayro_cmap::BfString;
use hayro::hayro_interpret::util::TransformExt;
use hayro::hayro_interpret::{
    BlendMode, ClipPath, Context, Device, DrawMode, DrawProps, Image, ImageDrawProps,
    InterpreterCache, InterpreterSettings, SoftMask, interpret_page,
};
use hayro::kurbo::{BezPath, Point, Rect, Vec2};
use lopdf::Document;
use std::collections::HashSet;

/// Glyph space: the interpreter scales every font to this many units per em.
const UNITS: f64 = 1000.0;
/// A gap this wide, in ems, is a space between words.
const WORD_GAP: f64 = 0.2;
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
        across <= size * LINE_STEP && along >= -size * 2.0
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
    fn draw_image(&mut self, _: Image<'a, '_>, _: ImageDrawProps<'a>) {}
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
        if !self.repeats(&found) {
            self.glyphs.append(&mut found);
            self.runs += 1;
        }
    }
}

/// Each character in drawing order with its glyph. A space or line break the
/// positions call for has no glyph.
fn reading(glyphs: &[Glyph]) -> Vec<(char, Option<usize>)> {
    let mut out: Vec<(char, Option<usize>)> = Vec::new();
    for (i, glyph) in glyphs.iter().enumerate() {
        if let Some(last) = i.checked_sub(1).map(|p| &glyphs[p]) {
            let blank = out.last().is_none_or(|(c, _)| c.is_whitespace());
            if !last.same_line(glyph) {
                out.push(('\n', None));
            } else if !blank && !glyph.is_blank() {
                let (along, _) = last.step_to(glyph);
                if along > last.size().max(glyph.size()) * WORD_GAP {
                    out.push((' ', None));
                }
            }
        }
        out.extend(glyph.text.chars().map(|c| (c, Some(i))));
    }
    out
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
                InterpreterSettings::default(),
            );
            let mut found = Collector::default();
            interpret_page(page, &mut context, &mut found);
            read(
                i,
                PageText {
                    glyphs: found.glyphs,
                    width: width as f64,
                    height: height as f64,
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
