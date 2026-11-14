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

/// What the job words must say about a command for it to be a hit at all. One
/// rare word in its name is plenty; a common word in its summary is not.
const MIN_JOB: f32 = 6.0;

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
];

/// Jobs people ask of files that no command does, where the other words of the
/// request match a command that only looks alike: redacting numbers is not
/// `pdf number`, cutting a person out of a photo is not `image transparent`,
/// subtitles are not `video watermark`.
const UNDONE: &[&str] = &[
    "anonymise",
    "anonymize",
    "blackout",
    "censor",
    "face",
    "faces",
    "filter",
    "people",
    "person",
    "query",
    "redact",
    "redaction",
    "sql",
    "srt",
    "subtitle",
    "subtitles",
];

/// Sentence words that say nothing about the job, on top of `STOP`.
const FILLER: &[&str] = &[
    "all", "are", "as", "at", "be", "but", "by", "called", "can", "do", "how", "i", "is", "it",
    "its", "must", "named", "need", "new", "not", "on", "one", "only", "save", "so", "that",
    "their", "them", "two", "version", "wanna", "want", "what", "you",
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

/// Words for a kind of file rather than one format: "a video", "the recording".
/// A kind with no formats is one deft reads none of, such as an archive.
type IsKind = fn(Format) -> bool;

const KINDS: &[(&str, IsKind)] = &[
    ("video", Format::is_video),
    ("videos", Format::is_video),
    ("clip", Format::is_video),
    ("clips", Format::is_video),
    ("footage", Format::is_video),
    ("movie", Format::is_video),
    ("movies", Format::is_video),
    ("audio", Format::is_audio),
    ("sound", Format::is_audio),
    ("song", Format::is_audio),
    ("songs", Format::is_audio),
    ("music", Format::is_audio),
    ("recording", Format::is_audio),
    ("recordings", Format::is_audio),
    ("podcast", Format::is_audio),
    ("podcasts", Format::is_audio),
    ("speech", Format::is_audio),
    ("voice", Format::is_audio),
    ("photo", Format::is_image),
    ("photos", Format::is_image),
    ("picture", Format::is_image),
    ("pictures", Format::is_image),
    ("image", Format::is_image),
    ("images", Format::is_image),
    ("screenshot", Format::is_image),
    ("screenshots", Format::is_image),
    ("powerpoint", |f| matches!(f, Format::Pptx | Format::Odp)),
    ("presentation", |f| matches!(f, Format::Pptx | Format::Odp)),
    ("excel", |f| matches!(f, Format::Xlsx | Format::Xls)),
    ("spreadsheet", is_sheet),
    ("spreadsheets", is_sheet),
    ("zip", |_| false),
    ("archive", |_| false),
    ("rar", |_| false),
    ("tar", |_| false),
];

fn is_sheet(f: Format) -> bool {
    matches!(f, Format::Xlsx | Format::Xls | Format::Ods | Format::Csv)
}

/// The formats a word names: "scan.png", "pdf", "word", "video". `None` when it
/// names no kind of file. "text" and "document" are left out: "the text of a
/// pdf" asks about content, not a .txt file.
fn named(token: &str) -> Option<Vec<Format>> {
    if let Some((_, is)) = KINDS.iter().find(|(w, _)| *w == token) {
        return Some(Format::ALL.iter().copied().filter(|f| is(*f)).collect());
    }
    if matches!(token, "text" | "txt" | "document" | "documents") {
        return None;
    }
    let format = match token.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => Format::from_name(ext),
        _ => Format::from_name(token).or_else(|| Format::from_name(&normalize(token))),
    };
    format.map(|f| vec![f])
}

/// The files a query talks about: the formats it starts from, and the formats
/// it asks for. A name after "to", "as" or "make a" is the output.
#[derive(Default, Debug, PartialEq)]
struct Files {
    inputs: Vec<Format>,
    outputs: Vec<Format>,
    /// Whether the query names an input or output at all, even one of no format
    /// deft reads ("an mp3", "a zip archive").
    named_input: bool,
    named_output: bool,
}

fn files(query: &str) -> Files {
    let is_output = |w: &str| matches!(w, "to" | "into" | "as" | "called" | "named");
    let tokens: Vec<String> = query
        .split_whitespace()
        .map(|t| {
            t.trim_matches(|c: char| !c.is_alphanumeric())
                .to_ascii_lowercase()
        })
        .collect();
    let mut files = Files::default();
    for (i, token) in tokens.iter().enumerate() {
        let before = |back: usize| i.checked_sub(back).map_or("", |j| tokens[j].as_str());
        let article = matches!(before(1), "a" | "an" | "the");
        let makes = matches!(before(1), "make" | "create")
            || (matches!(before(1), "a" | "an") && matches!(before(2), "make" | "create"));
        // "zip my files" is a verb.
        if token == "zip" && i == 0 {
            continue;
        }
        let Some(formats) = named(token) else {
            continue;
        };
        if is_output(before(1)) || (article && is_output(before(2))) || makes {
            files.named_output = true;
            files.outputs.extend(formats);
        } else {
            files.named_input = true;
            files.inputs.extend(formats);
        }
    }
    files
}

/// Formats of the input the query talks about.
#[cfg(test)]
fn inputs(query: &str) -> Vec<Format> {
    files(query).inputs
}

/// Whether an operation fits the files the query names: it reads one of the
/// inputs and writes one of the outputs. "compress a video" is no job for
/// `pdf compress`, however well the verb matches.
fn fits(op: &Op, files: &Files) -> bool {
    let reads = !files.named_input || files.inputs.iter().any(|f| op.input.accepts(*f));
    let writes = !files.named_output
        || op.output.formats.is_empty()
        || files.outputs.iter().any(|f| op.output.formats.contains(f));
    reads && writes
}

/// Whether a word names a kind of file rather than a job. A QR code or barcode
/// is what the job makes or reads, so `qr` is the one group that is a job.
fn names_a_type(reg: &Registry, w: &str) -> bool {
    reg.group(w).is_some_and(|g| g.name != "qr") || named(w).is_some() || FILE_WORDS.contains(&w)
}

/// The single operation for a query such as "png to jpg" or "turn a word
/// document into markdown", if there is one.
fn conversion(reg: &Registry, files: &Files) -> Option<&'static Op> {
    files.inputs.iter().find_map(|from| {
        files
            .outputs
            .iter()
            .find_map(|to| match reg.plan_conversion(*from, *to)?.as_slice() {
                [(op, _)] => Some(*op),
                _ => None,
            })
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
        "min_job": MIN_JOB,
        "undone": UNDONE,
        "kinds": KINDS
            .iter()
            .map(|(w, is)| {
                let formats: Vec<_> = Format::ALL.iter().filter(|f| is(**f)).map(|f| f.ext()).collect();
                (w.to_string(), json!(formats))
            })
            .collect::<Map<_, _>>(),
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
    let names_a_type = |w: &str| names_a_type(reg, w);
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

/// Search commands by intent. Returns (path, op, score), best first, and
/// nothing when no command does the job.
pub(super) fn find(reg: &Registry, query: &str) -> Vec<(String, &'static Op, u32)> {
    let index = index(reg);

    let all: Vec<String> = words(query).collect();
    let mut scores = vec![0.0f32; index.entries.len()];
    // What the words that name the job, not a kind of file, say for each entry.
    let mut job = vec![0.0f32; index.entries.len()];
    let mut job_words = 0;
    for word in all
        .iter()
        .filter(|w| !STOP.contains(&w.as_str()) && !FILLER.contains(&w.as_str()))
    {
        let is_job = !names_a_type(reg, word);
        job_words += is_job as usize;
        let mut best: HashMap<usize, (f32, f32)> = HashMap::new();
        for (term, sure) in index.terms(word) {
            let rarity = index.rarity(&term);
            for (entry, weight) in index.postings.get(&term).into_iter().flatten() {
                let score = *weight as f32 * rarity * sure;
                // Only a word in the command's name, tags or summary says what it
                // does; a word in an option's help says what it can be told.
                // A word of its name is enough, however many commands share it.
                let says = match *weight {
                    _ if sure <= DOUBTFUL => 0.0,
                    COMMAND => score.max(MIN_JOB),
                    w if w >= SUMMARY => score,
                    _ => 0.0,
                };
                let slot = best.entry(*entry).or_default();
                slot.0 = slot.0.max(score);
                slot.1 = slot.1.max(says);
            }
        }
        for (entry, (score, says)) in best {
            scores[entry] += score;
            if is_job {
                job[entry] += says;
            }
        }
    }

    let undone = all.iter().any(|w| UNDONE.contains(&w.as_str()))
        || all.windows(2).any(|w| w[0] == "black" && w[1] == "out");
    if undone {
        return vec![];
    }

    let files = files(query);
    // A format with its own group is handled there: "merge two json files" is
    // not a job for `sheet merge`, which also reads JSON rows.
    let homes: Vec<&str> = files
        .inputs
        .iter()
        .filter_map(|f| reg.group(f.ext()))
        .map(|g| g.name)
        .collect();
    for (entry, score) in scores.iter_mut().enumerate() {
        let (path, op) = &index.entries[entry];
        let group = path.split(' ').next().unwrap_or_default();
        if !homes.is_empty() && !homes.contains(&group) {
            *score = 0.0;
        }
        if files.inputs.iter().any(|f| op.input.accepts(*f)) {
            *score *= ACCEPTS_INPUT;
        }
        // A query that names a job keeps only commands that do it, on the
        // files it names. A query of file types alone ("pdf") lists them all.
        if !fits(op, &files) || (job_words > 0 && job[entry] < MIN_JOB) {
            *score = 0.0;
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

    if let Some(op) = conversion(reg, &files) {
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
