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
    let mut fence: Option<(u8, usize)> = None;
    for line in body.lines() {
        if let Some((marker, length, closes)) = fence_marker(line) {
            match fence {
                Some((open_marker, open_length))
                    if marker == open_marker && length >= open_length && closes =>
                {
                    fence = None
                }
                None => fence = Some((marker, length)),
                _ => {}
            }
            continue;
        }
        if fence.is_some() {
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

fn fence_marker(line: &str) -> Option<(u8, usize, bool)> {
    let trimmed = line.trim_start();
    let marker = *trimmed.as_bytes().first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let length = trimmed.bytes().take_while(|byte| *byte == marker).count();
    let closes = trimmed[length..].trim().is_empty();
    (length >= 3).then_some((marker, length, closes))
}

/// The pointers of one line, outside inline code spans.
fn line_pointers(line: &str) -> Vec<String> {
    let mut found = Vec::new();
    let bytes = line.as_bytes();
    let mut outside_start = 0;
    let mut cursor = 0;
    while cursor < bytes.len() {
        if bytes[cursor] != b'`' {
            cursor += 1;
            continue;
        }
        let run_start = cursor;
        while cursor < bytes.len() && bytes[cursor] == b'`' {
            cursor += 1;
        }
        let run_length = cursor - run_start;
        let mut search = cursor;
        let mut close_end = None;
        while search < bytes.len() {
            let Some(offset) = line[search..].find('`') else {
                break;
            };
            let start = search + offset;
            let mut end = start;
            while end < bytes.len() && bytes[end] == b'`' {
                end += 1;
            }
            if end - start == run_length {
                close_end = Some(end);
                break;
            }
            search = end;
        }
        if let Some(end) = close_end {
            collect_pointers(&line[outside_start..run_start], &mut found);
            outside_start = end;
            cursor = end;
        }
    }
    collect_pointers(&line[outside_start..], &mut found);
    found
}

fn collect_pointers(part: &str, found: &mut Vec<String>) {
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
