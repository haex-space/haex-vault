//! Pointers between pages (spec 097 FR-029, data-model.md "Pointers"): every `[[Title]]` in a
//! page's text, `[[Title|shown text]]` included, outside code. Nothing about them is stored; the
//! map of pointers out of and into a page is built from the texts when it is needed.

use std::collections::{HashMap, HashSet};

use super::title_key;

#[cfg(test)]
#[path = "links_tests.rs"]
mod tests;

/// The titles `body` points to, in order, each once (compared like titles).
pub fn pointers(body: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut seen = HashSet::new();
    let mut fenced = false;
    for line in body.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        for title in line_pointers(line) {
            if seen.insert(title_key(&title)) {
                found.push(title);
            }
        }
    }
    found
}

/// The pointers of one line, outside inline code spans.
fn line_pointers(line: &str) -> Vec<String> {
    let mut found = Vec::new();
    for (index, part) in line.split('`').enumerate() {
        if index % 2 == 1 {
            continue;
        }
        let mut rest = part;
        while let Some(start) = rest.find("[[") {
            let after = &rest[start + 2..];
            let Some(end) = after.find("]]") else { break };
            let inner = &after[..end];
            let title = inner.split('|').next().unwrap_or("").trim();
            if !title.is_empty() {
                found.push(title.to_owned());
            }
            rest = &after[end + 2..];
        }
    }
    found
}

/// A pointer out of a page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pointer {
    /// The title as written in the text.
    pub title: String,
    /// No page has that title.
    pub missing: bool,
}

/// The pointers out of and into every page of the memory.
#[derive(Debug, Default)]
pub struct LinkMap {
    out: HashMap<String, Vec<String>>,
    into: HashMap<String, Vec<String>>,
}

impl LinkMap {
    /// Builds the map from `(title, body)` of every page, changed last first: of two pages with one
    /// title (a sync can make them) the first counts, as a title resolves to it.
    pub fn build<'a>(pages: impl IntoIterator<Item = (&'a str, &'a str)>) -> Self {
        let mut map = Self::default();
        for (title, body) in pages {
            let key = title_key(title);
            if map.out.contains_key(&key) {
                continue;
            }
            let targets = pointers(body);
            for target in &targets {
                let target_key = title_key(target);
                if target_key == key {
                    continue;
                }
                let into = map.into.entry(target_key).or_default();
                if !into.iter().any(|from| title_key(from) == key) {
                    into.push(title.to_owned());
                }
            }
            map.out.entry(key).or_default().extend(targets);
        }
        map
    }

    /// The pointers out of the page `title`, each marked when no page has its title.
    pub fn points_to(&self, title: &str) -> Vec<Pointer> {
        self.out
            .get(&title_key(title))
            .map(|targets| {
                targets
                    .iter()
                    .map(|target| Pointer {
                        title: target.clone(),
                        missing: !self.out.contains_key(&title_key(target)),
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The titles of the pages that point to `title`.
    pub fn pointed_from(&self, title: &str) -> Vec<String> {
        self.into
            .get(&title_key(title))
            .cloned()
            .unwrap_or_default()
    }
}
