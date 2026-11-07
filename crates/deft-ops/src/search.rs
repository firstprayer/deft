//! `deft find`: ranked search over commands.
//!
//! An inverted index maps every searchable word to the commands it describes.
//! It is built on the first search, so no other command pays for it. Rare words
//! count for more than common ones, and a word that is not in the index is
//! corrected to the nearest word that is.

use super::{GLOBAL_SYNONYMS, Registry, STOP, WORD_FORMS, distance, normalize};
use deft_core::format::Format;
use deft_core::spec::Op;
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::sync::OnceLock;

/// How much a word says about a command, by where it was found.
const COMMAND: u32 = 6;
const GROUP: u32 = 5;
const SYNONYM: u32 = 5;
const TAG: u32 = 4;
const SUMMARY: u32 = 3;
const OPTION: u32 = 2;
const OPTION_HELP: u32 = 1;

/// How much a command gains by accepting the file the query names.
const ACCEPTS_INPUT: f32 = 1.5;

/// A hit must score at least this share of the best hit, in percent.
const KEEP: u32 = 60;
const MAX_HITS: usize = 5;

/// How sure a spelling correction two letters away is. See `corrections`.
const DOUBTFUL: f32 = 0.5;

/// Words for a kind of file, which name no job.
const FILE_WORDS: &[&str] = &[
    "audio",
    "recording",
    "sound",
    "document",
    "documents",
    "file",
    "files",
    "photo",
    "photos",
    "picture",
    "pictures",
    "text",
];

/// Sentence words that say nothing about the job, on top of `STOP`.
const FILLER: &[&str] = &[
    "all", "are", "as", "at", "be", "but", "by", "called", "can", "do", "how", "i", "is", "it",
    "its", "must", "named", "need", "new", "not", "on", "one", "only", "save", "so", "that",
    "their", "them", "version", "want", "what", "you",
];

struct Index {
    /// (command path, operation), sorted by path.
    entries: Vec<(String, &'static Op)>,
    /// Word -> (entry, weight).
    postings: HashMap<String, Vec<(usize, u32)>>,
    /// (spelling, indexed word): what a misspelled word can be corrected to.
    spellings: Vec<(String, String)>,
}

fn words(text: &str) -> impl Iterator<Item = String> + '_ {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_ascii_lowercase)
}

impl Index {
    fn build(reg: &Registry) -> Index {
        let entries = reg.all_paths();
        let mut postings: HashMap<String, Vec<(usize, u32)>> = HashMap::new();
        let mut spellings: HashMap<String, String> = HashMap::new();
        for (entry, (path, op)) in entries.iter().enumerate() {
            let (group, command) = path.split_once(' ').unwrap_or((path, ""));
            let mut weights: HashMap<String, u32> = HashMap::new();
            let mut add = |word: &str, weight: u32| {
                if STOP.contains(&word) || FILLER.contains(&word) {
                    return;
                }
                let term = normalize(word);
                spellings.insert(word.to_string(), term.clone());
                let w = weights.entry(term).or_default();
                *w = (*w).max(weight);
            };
            for word in words(command) {
                add(&word, COMMAND);
            }
            add(group, GROUP);
            let group_def = reg.group(group);
            for alias in group_def.iter().flat_map(|g| g.aliases) {
                add(alias, GROUP);
            }
            let names = |to: &str| command.split('-').any(|c| c == to);
            for (from, to) in group_def.iter().flat_map(|g| g.synonyms) {
                if names(to) {
                    add(from, TAG);
                }
            }
            for (from, to) in GLOBAL_SYNONYMS {
                if names(to) {
                    add(from, SYNONYM);
                }
            }
            for word in op.tags.iter().flat_map(|t| words(t)) {
                add(&word, TAG);
            }
            for word in words(op.summary) {
                add(&word, SUMMARY);
            }
            for param in op.params {
                for word in words(param.name) {
                    add(&word, OPTION);
                }
                for word in words(param.help) {
                    add(&word, OPTION_HELP);
                }
            }
            for (term, weight) in weights {
                postings.entry(term).or_default().push((entry, weight));
            }
        }
        for (from, _) in WORD_FORMS {
            let term = normalize(from);
            if postings.contains_key(&term) {
                spellings.insert(from.to_string(), term);
            }
        }
        let mut spellings: Vec<_> = spellings.into_iter().collect();
        spellings.sort();
        Index {
            entries,
            postings,
            spellings,
        }
    }

    /// Rare words decide; a word on every command decides nothing.
    fn rarity(&self, term: &str) -> f32 {
        let found = self.postings.get(term).map_or(0, Vec::len).max(1);
        (1.0 + self.entries.len() as f32 / found as f32).ln()
    }

    /// Indexed words a query word stands for, with how sure that reading is.
    fn terms(&self, word: &str) -> Vec<(String, f32)> {
        let stems = stems(word);
        let mut terms: Vec<(String, f32)> = vec![];
        let readings = std::iter::once((word, 1.0)).chain(stems.iter().map(|s| (s.as_str(), 0.95)));
        for (reading, sure) in readings {
            let term = normalize(reading);
            if self.postings.contains_key(&term) && !terms.iter().any(|t| t.0 == term) {
                terms.push((term, sure));
            }
        }
        if terms.is_empty() {
            return self.corrections(word, &stems);
        }
        terms
    }

    /// The nearest spellings to a word that is not in the index.
    fn corrections(&self, word: &str, stems: &[String]) -> Vec<(String, f32)> {
        if word.len() < 4 || word.starts_with(|c: char| c.is_ascii_digit()) {
            return vec![];
        }
        let allowed = if word.len() >= 6 { 2 } else { 1 };
        let mut best = allowed + 1;
        let mut found: Vec<String> = vec![];
        for (spelling, term) in &self.spellings {
            let near = stems
                .iter()
                .map(|s| distance(s, spelling) + 1)
                .chain([distance(word, spelling)])
                .min()
                .unwrap_or(usize::MAX);
            let near = if spelling.starts_with(word) {
                near.min(1)
            } else {
                near
            };
            if near < best {
                best = near;
                found.clear();
            }
            if near == best && !found.contains(term) {
                found.push(term.clone());
            }
        }
        let sure = if best <= 1 { 0.7 } else { DOUBTFUL };
        found.into_iter().map(|t| (t, sure)).collect()
    }
}

/// A word without its inflection: "resizing" gives "resiz" and "resize", and
/// "padding" gives "pad" as well.
fn stems(word: &str) -> Vec<String> {
    let mut out = vec![];
    for ending in ["ing", "ed", "es", "s"] {
        if let Some(stem) = word.strip_suffix(ending).filter(|s| s.len() >= 3) {
            out.push(stem.to_string());
            out.push(format!("{stem}e"));
            let mut letters = stem.chars().rev();
            if letters.next() == letters.next() {
                out.push(stem[..stem.len() - 1].to_string());
            }
        }
    }
    out
}

/// Formats of the input the query talks about: `scan.png`, or "pdf" alone. A
/// name after "to" or "as" is the output, and "zip" alone is usually a verb.
fn inputs(query: &str) -> Vec<Format> {
    let is_output = |w: &str| matches!(w, "to" | "into" | "as" | "called" | "named");
    let tokens: Vec<String> = query
        .split_whitespace()
        .map(|t| {
            t.trim_matches(|c: char| !c.is_alphanumeric())
                .to_ascii_lowercase()
        })
        .collect();
    let mut formats = vec![];
    for (i, token) in tokens.iter().enumerate() {
        let before = |back: usize| i.checked_sub(back).map_or("", |j| tokens[j].as_str());
        let article = matches!(before(1), "a" | "an" | "the");
        if is_output(before(1)) || (article && is_output(before(2))) {
            continue;
        }
        let format = match token.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() => Format::from_name(ext),
            _ if token == "zip" => None,
            _ => Format::from_name(token).or_else(|| Format::from_name(&normalize(token))),
        };
        formats.extend(format);
    }
    formats
}

/// The single operation for a query such as "png to jpg", if there is one.
fn conversion(reg: &Registry, query: &[String]) -> Option<&'static Op> {
    let format = |w: &str| Format::from_name(w).or_else(|| Format::from_name(&normalize(w)));
    query.windows(3).find_map(|w| {
        if !matches!(w[1].as_str(), "to" | "into" | "as") {
            return None;
        }
        let plan = reg.plan_conversion(format(&w[0])?, format(&w[2])?)?;
        match plan.as_slice() {
            [(op, _)] => Some(*op),
            _ => None,
        }
    })
}

fn index(reg: &Registry) -> &'static Index {
    static INDEX: OnceLock<Index> = OnceLock::new();
    INDEX.get_or_init(|| Index::build(reg))
}

/// The index as data, for the homepage's copy of `find`, so that page never
/// needs its own idea of what a command is about.
pub(super) fn export(reg: &Registry) -> Value {
    let index = index(reg);
    let mut pairs = Map::new();
    for from in Format::ALL {
        let mut targets = Map::new();
        for to in Format::ALL {
            if let Some([(op, _)]) = reg.plan_conversion(*from, *to).as_deref() {
                targets.insert(to.ext().to_string(), json!(op.id));
            }
        }
        pairs.insert(from.ext().to_string(), Value::Object(targets));
    }
    let mut accepts = Map::new();
    for op in &reg.ops {
        let formats = Format::ALL.iter().filter(|f| op.input.accepts(**f));
        accepts.insert(
            op.id.to_string(),
            json!(formats.map(|f| f.ext()).collect::<Vec<_>>()),
        );
    }
    let mut postings: Vec<_> = index.postings.iter().collect();
    postings.sort();
    json!({
        "entries": index.entries.iter().map(|(p, op)| json!([p, op.id])).collect::<Vec<_>>(),
        "postings": postings,
        "spellings": index.spellings,
        "skip": STOP.iter().chain(FILLER).collect::<Vec<_>>(),
        "forms": WORD_FORMS,
        "pairs": pairs,
        "accepts": accepts,
        "boost": ACCEPTS_INPUT,
        "keep": KEEP,
        "max_hits": MAX_HITS,
    })
}

/// Words that name the job when no command knows them. A query whose only known
/// words are file types has found nothing for the job itself, however good its
/// hits look: "translate pdf" matches every PDF command. Empty when hits can be
/// trusted.
pub(super) fn unknown_job(reg: &Registry, query: &str) -> Vec<String> {
    let index = index(reg);
    let job: Vec<String> = words(query)
        .filter(|w| !STOP.contains(&w.as_str()) && !FILLER.contains(&w.as_str()))
        .collect();
    // The index is English, so it cannot tell what another language asks for.
    if job.iter().any(|w| !w.is_ascii()) {
        return vec![];
    }
    let names_a_type = |w: &str| {
        reg.group(w).is_some()
            || Format::from_name(&normalize(w)).is_some()
            || FILE_WORDS.contains(&w)
    };
    // A guess two letters away says nothing: it reads "translate" as "transpose".
    // Swapped letters are the exception, because "rezise" is plainly a typo.
    let letters = |w: &str| {
        let mut l: Vec<char> = w.chars().collect();
        l.sort_unstable();
        l
    };
    let spelled = |w: &str| {
        let terms = index.terms(w);
        terms
            .iter()
            .any(|t| t.1 > DOUBTFUL || letters(&t.0) == letters(w))
    };
    let (known, unknown): (Vec<_>, Vec<_>) =
        job.into_iter().partition(|w| names_a_type(w) || spelled(w));
    if known.iter().all(|w| names_a_type(w)) {
        unknown
    } else {
        vec![]
    }
}

/// Search commands by intent. Returns (path, op, score), best first.
pub(super) fn find(reg: &Registry, query: &str) -> Vec<(String, &'static Op, u32)> {
    let index = index(reg);

    let all: Vec<String> = words(query).collect();
    let mut scores = vec![0.0f32; index.entries.len()];
    for word in all
        .iter()
        .filter(|w| !STOP.contains(&w.as_str()) && !FILLER.contains(&w.as_str()))
    {
        let mut best: HashMap<usize, f32> = HashMap::new();
        for (term, sure) in index.terms(word) {
            let rarity = index.rarity(&term);
            for (entry, weight) in index.postings.get(&term).into_iter().flatten() {
                let score = *weight as f32 * rarity * sure;
                let slot = best.entry(*entry).or_default();
                *slot = slot.max(score);
            }
        }
        for (entry, score) in best {
            scores[entry] += score;
        }
    }

    let inputs = inputs(query);
    for (entry, score) in scores.iter_mut().enumerate() {
        if inputs
            .iter()
            .any(|f| index.entries[entry].1.input.accepts(*f))
        {
            *score *= ACCEPTS_INPUT;
        }
    }

    // One hit per operation, under its canonical path where that scores.
    let mut best: HashMap<&'static str, (String, &'static Op, u32)> = HashMap::new();
    for (entry, score) in scores.iter().enumerate() {
        if *score <= 0.0 {
            continue;
        }
        let (path, op) = &index.entries[entry];
        let score = (*score * 100.0) as u32 * 2 + (op.path() == *path) as u32;
        let hit = best.entry(op.id).or_insert((path.clone(), op, 0));
        if score > hit.2 {
            *hit = (path.clone(), op, score);
        }
    }
    let mut hits: Vec<_> = best.into_values().collect();
    hits.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));

    if let Some(op) = conversion(reg, &all) {
        let top = hits.first().map_or(1, |h| h.2);
        hits.retain(|h| h.1.id != op.id);
        hits.insert(0, (op.path(), op, top + 1));
    }

    let top = hits.first().map_or(0, |h| h.2);
    hits.retain(|h| h.2 as u64 * 100 >= top as u64 * KEEP as u64);
    hits.truncate(MAX_HITS);
    hits
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
