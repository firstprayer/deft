//! Lookups over the generated registry: command paths, aliases, verb
//! synonyms, search and the conversion graph.

use crate::generated::{GROUPS, OPS};
use deft_core::format::Format;
use deft_core::spec::{Group, Op};
use std::collections::{BinaryHeap, HashMap};
use std::sync::OnceLock;

#[path = "search.rs"]
mod search;

pub struct Registry {
    pub groups: Vec<&'static Group>,
    pub ops: Vec<&'static Op>,
    /// (group, command) -> operation, for every canonical path and mount.
    paths: HashMap<(String, String), &'static Op>,
}

/// How a command path was resolved.
pub struct Resolved {
    pub op: &'static Op,
    /// The path the user reached it by (after alias resolution).
    pub path: String,
}

/// Verb synonyms that apply in every group.
pub const GLOBAL_SYNONYMS: &[(&str, &str)] = &[
    ("combine", "merge"),
    ("join", "merge"),
    ("concat", "merge"),
    ("concatenate", "merge"),
    ("shrink", "compress"),
    ("optimize", "compress"),
    ("minify", "compress"),
    ("reduce", "compress"),
    ("thumbnail", "resize"),
    ("scale", "resize"),
    ("trim", "split"),
    ("cut", "split"),
    ("clip", "split"),
    ("unzip", "extract"),
    ("decompress", "extract"),
    ("ls", "list"),
    ("meta", "info"),
    ("probe", "info"),
    ("inspect", "info"),
    ("stats", "info"),
    ("remove", "delete"),
    ("pages", "select"),
    ("reorder", "select"),
    ("turn", "rotate"),
    ("mirror", "flip"),
    ("transcode", "convert"),
    ("export", "convert"),
    ("get", "download"),
    ("fetch", "download"),
    ("captions", "subtitles"),
    ("subs", "subtitles"),
    ("transcript", "subtitles"),
    ("cover", "thumbnail"),
];

pub fn registry() -> &'static Registry {
    static R: OnceLock<Registry> = OnceLock::new();
    R.get_or_init(|| {
        let mut groups: Vec<&'static Group> = GROUPS.to_vec();
        groups.sort_by_key(|g| (g.order, g.name));
        let mut paths = HashMap::new();
        for op in OPS {
            for path in op.paths() {
                if let Some((g, c)) = path.split_once(' ') {
                    paths.insert((g.to_string(), c.to_string()), *op);
                }
            }
        }
        Registry {
            groups,
            ops: OPS.to_vec(),
            paths,
        }
    })
}

impl Registry {
    pub fn group(&self, name: &str) -> Option<&'static Group> {
        let name = name.to_ascii_lowercase();
        self.groups
            .iter()
            .copied()
            .find(|g| g.name == name || g.aliases.contains(&name.as_str()))
    }

    pub fn op(&self, id: &str) -> Option<&'static Op> {
        self.ops.iter().copied().find(|o| o.id == id)
    }

    /// Resolve `group command`, applying group aliases and verb synonyms.
    pub fn resolve(&self, group: &str, command: &str) -> Option<Resolved> {
        let g = self.group(group)?;
        let command = command.to_ascii_lowercase();
        let lookup = |c: &str| {
            self.paths
                .get(&(g.name.to_string(), c.to_string()))
                .copied()
        };
        if let Some(op) = lookup(&command) {
            return Some(Resolved {
                op,
                path: format!("{} {}", g.name, command),
            });
        }
        let synonym = g
            .synonyms
            .iter()
            .chain(GLOBAL_SYNONYMS)
            .find(|(from, _)| *from == command)
            .map(|(_, to)| *to)?;
        lookup(synonym).map(|op| Resolved {
            op,
            path: format!("{} {}", g.name, synonym),
        })
    }

    /// Commands listed under a group: (command name, operation), including mounts.
    pub fn commands(&self, group: &str) -> Vec<(String, &'static Op)> {
        let mut v: Vec<(String, &'static Op)> = self
            .paths
            .iter()
            .filter(|((g, _), _)| g == group)
            .map(|((_, c), op)| (c.clone(), *op))
            .collect();
        v.sort_by(|a, b| (a.0 != "info", &a.0).cmp(&(b.0 != "info", &b.0)));
        v
    }

    /// Every (path, op) pair, canonical paths first.
    pub fn all_paths(&self) -> Vec<(String, &'static Op)> {
        let mut v: Vec<_> = self
            .paths
            .iter()
            .map(|((g, c), op)| (format!("{g} {c}"), *op))
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    }

    /// Close matches for an unknown group or command, for "did you mean".
    pub fn suggest(&self, group: Option<&str>, word: &str) -> Vec<String> {
        let word = word.to_ascii_lowercase();
        let mut scored: Vec<(usize, String)> = Vec::new();
        match group.and_then(|g| self.group(g)) {
            Some(g) => {
                for (c, _) in self.commands(g.name) {
                    let d = distance(&word, &c);
                    if d <= 2 || c.starts_with(&word) || word.starts_with(&c) {
                        scored.push((d, format!("deft {} {c}", g.name)));
                    }
                }
                // The same verb in other groups ("deft image merge" -> "deft pdf from-images").
                for (path, _) in self.all_paths() {
                    let (pg, pc) = path.split_once(' ').unwrap();
                    if pg != g.name && (pc == word || distance(pc, &word) <= 1) {
                        scored.push((3, format!("deft {path}")));
                    }
                }
                for hit in self.find(&format!("{} {word}", g.name)).into_iter().take(2) {
                    scored.push((4, format!("deft {}", hit.0)));
                }
            }
            None => {
                for g in &self.groups {
                    for name in std::iter::once(&g.name).chain(g.aliases.iter()) {
                        let d = distance(&word, name);
                        if d <= 2 {
                            scored.push((d, format!("deft {}", g.name)));
                        }
                    }
                }
                for hit in self.find(&word).into_iter().take(3) {
                    scored.push((4, format!("deft {}", hit.0)));
                }
            }
        }
        scored.sort();
        let mut out: Vec<String> = Vec::new();
        for (_, s) in scored {
            if !out.contains(&s) {
                out.push(s);
            }
        }
        out.truncate(3);
        out
    }

    /// Search commands by intent. Returns (path, op, score), best first.
    pub fn find(&self, query: &str) -> Vec<(String, &'static Op, u32)> {
        search::find(self, query)
    }

    /// Words of a query that name a job no command knows. See `search`.
    pub fn unknown_job(&self, query: &str) -> Vec<String> {
        search::unknown_job(self, query)
    }

    /// The search index as data, for pages that search without the binary.
    pub fn search_index(&self) -> serde_json::Value {
        search::export(self)
    }

    /// Cheapest chain of conversion operations from one format to another.
    pub fn plan_conversion(&self, from: Format, to: Format) -> Option<Vec<(&'static Op, Format)>> {
        if from == to {
            return Some(vec![]);
        }
        // Dijkstra over formats. Each hop costs its quality cost plus one.
        let mut dist: HashMap<Format, u32> = HashMap::new();
        let mut prev: HashMap<Format, (Format, &'static Op)> = HashMap::new();
        let mut heap = BinaryHeap::new();
        dist.insert(from, 0);
        heap.push(std::cmp::Reverse((0u32, from)));
        while let Some(std::cmp::Reverse((d, f))) = heap.pop() {
            if f == to {
                break;
            }
            if dist.get(&f).is_some_and(|best| d > *best) {
                continue;
            }
            for op in &self.ops {
                for edge in op.converts {
                    if !edge.from.contains(&f) {
                        continue;
                    }
                    for &next in edge.to {
                        if next == f {
                            continue;
                        }
                        let nd = d + edge.quality.cost() + 1;
                        if dist.get(&next).is_none_or(|old| nd < *old) {
                            dist.insert(next, nd);
                            prev.insert(next, (f, *op));
                            heap.push(std::cmp::Reverse((nd, next)));
                        }
                    }
                }
            }
        }
        if !prev.contains_key(&to) {
            return None;
        }
        let mut chain = Vec::new();
        let mut cur = to;
        while cur != from {
            let (p, op) = prev[&cur];
            chain.push((op, cur));
            cur = p;
        }
        chain.reverse();
        // Long chains are accidents of the graph, such as a table to MP3 by way
        // of a picture and a video, not conversions anyone wants.
        (chain.len() <= 3).then_some(chain)
    }

    /// Formats reachable from `from` by `deft convert`.
    pub fn reachable(&self, from: Format) -> Vec<Format> {
        Format::ALL
            .iter()
            .copied()
            .filter(|&t| t != from && self.plan_conversion(from, t).is_some())
            .collect()
    }
}

/// Words `find` ignores.
pub const STOP: &[&str] = &[
    "a", "an", "the", "to", "of", "for", "in", "into", "my", "this", "and", "with", "from", "file",
    "files", "make", "please", "me",
];

/// Words `find` treats as the same word: (word, the form it is indexed under).
pub const WORD_FORMS: &[(&str, &str)] = &[
    ("jpeg", "jpg"),
    ("picture", "image"),
    ("pictures", "image"),
    ("photo", "image"),
    ("photos", "image"),
    ("img", "image"),
    ("png", "image"),
    ("word", "docx"),
    ("excel", "sheet"),
    ("spreadsheet", "sheet"),
    ("markdown", "md"),
    ("smaller", "compress"),
    ("small", "compress"),
    ("shrink", "compress"),
    ("combine", "merge"),
    ("join", "merge"),
    ("mp3", "audio"),
    ("sound", "audio"),
    ("mp4", "video"),
    ("movie", "video"),
    ("clip", "video"),
    ("zip", "archive"),
    ("yt", "youtube"),
    ("caption", "subtitles"),
    ("captions", "subtitles"),
    ("transcript", "subtitles"),
];

/// Crude stemming so "pages" matches "page" and "images" matches "image".
fn normalize(w: &str) -> String {
    let w = w.to_ascii_lowercase();
    let w = WORD_FORMS
        .iter()
        .find(|(from, _)| *from == w)
        .map_or(w.clone(), |(_, to)| to.to_string());
    if w.len() > 4 && w.ends_with('s') && !w.ends_with("ss") && w != "subtitles" {
        w[..w.len() - 1].to_string()
    } else {
        w
    }
}

/// Levenshtein distance.
pub fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for j in 0..b.len() {
            let cur = row[j + 1];
            row[j + 1] = if ca == b[j] {
                prev
            } else {
                1 + prev.min(row[j]).min(row[j + 1])
            };
            prev = cur;
        }
    }
    row[b.len()]
}
