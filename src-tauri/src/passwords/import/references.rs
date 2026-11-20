//! KeePass references on import (spec 036, FR-050, research R13, `contracts/references.md`
//! §KeePass-Import): `{REF:<Feld>@<Suche>:<Text>}` becomes a haex-vault placeholder when it asks for the
//! user name (`U`) or the password (`P`) of exactly one entry of the file that is imported too.
//! Searching by id (`I`, 32 hex digits) or by text in the title, user name, password, address, notes
//! or a custom field (`T`, `U`, `P`, `A`, `N`, `O`, case ignored) is understood; anything else, an
//! ambiguous or missing match and a source skipped as a duplicate stay text and are counted. Pure.

use std::collections::HashMap;
use std::convert::Infallible;

use super::ImportItem;
use crate::passwords::references::{build_token, find_cycle, Field, RefKind};

/// A text field of an imported entry that may hold references.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportField {
    Username,
    Password,
    Url,
    Note,
    /// The custom field at this position.
    KeyValue(usize),
}

/// The new texts and the counts of a conversion.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Conversion {
    /// Entry (position in the model), field and its new text.
    pub rewrites: Vec<(usize, ImportField, String)>,
    pub converted: u32,
    pub left_as_text: u32,
}

/// One `{REF:…}` in a text: its byte range, the wanted field letter, the search letter and text.
struct KeepassRef<'a> {
    start: usize,
    end: usize,
    wanted: char,
    search: char,
    text: &'a str,
}

fn find_refs(text: &str) -> Vec<KeepassRef<'_>> {
    let mut found = Vec::new();
    let upper = text.to_ascii_uppercase();
    let mut from = 0;
    while let Some(offset) = upper[from..].find("{REF:") {
        let start = from + offset;
        let body_start = start + 5;
        let Some(close) = text[body_start..].find('}') else {
            break;
        };
        let body = &text[body_start..body_start + close];
        let mut chars = body.chars();
        let parsed = match (chars.next(), chars.next(), chars.next(), chars.next()) {
            // Only ASCII letters are supported, and only then is `body[4..]` a char boundary.
            (Some(wanted), Some('@'), Some(search), Some(':'))
                if wanted.is_ascii() && search.is_ascii() =>
            {
                Some(KeepassRef {
                    start,
                    end: body_start + close + 1,
                    wanted: wanted.to_ascii_uppercase(),
                    search: search.to_ascii_uppercase(),
                    text: &body[4..],
                })
            }
            _ => None,
        };
        match parsed {
            Some(reference) => {
                from = reference.end;
                found.push(reference);
            }
            None => from = start + 1,
        }
    }
    found
}

fn texts(item: &ImportItem) -> Vec<(ImportField, &str)> {
    let mut out = Vec::new();
    for (field, text) in [
        (ImportField::Username, &item.username),
        (ImportField::Password, &item.password),
        (ImportField::Url, &item.url),
        (ImportField::Note, &item.note),
    ] {
        if let Some(text) = text {
            out.push((field, text.as_str()));
        }
    }
    for (index, field) in item.key_values.iter().enumerate() {
        if let Some(value) = &field.value {
            out.push((ImportField::KeyValue(index), value.as_str()));
        }
    }
    out
}

fn contains(haystack: &Option<String>, needle: &str) -> bool {
    haystack
        .as_deref()
        .is_some_and(|text| text.to_lowercase().contains(needle))
}

/// The one entry a reference names, by position; `None` for no match or more than one.
fn source_of(items: &[ImportItem], reference: &KeepassRef<'_>) -> Option<usize> {
    let needle = reference.text.to_lowercase();
    let matches: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, item)| match reference.search {
            'I' => {
                let wanted = needle.replace('-', "");
                wanted.len() == 32
                    && item
                        .source_ref
                        .as_deref()
                        .is_some_and(|id| id.replace('-', "").to_lowercase() == wanted)
            }
            'T' => contains(&item.title, &needle),
            'U' => contains(&item.username, &needle),
            'P' => contains(&item.password, &needle),
            'A' => contains(&item.url, &needle),
            'N' => contains(&item.note, &needle),
            'O' => item
                .key_values
                .iter()
                .any(|kv| contains(&kv.value, &needle)),
            _ => false,
        })
        .map(|(index, _)| index)
        .collect();
    match matches.as_slice() {
        [one] => Some(*one),
        _ => None,
    }
}

/// The new texts of the entries with KeePass references. `ids` holds the id each entry gets, `None`
/// for one that is not written (a skipped duplicate): a reference to it stays text.
pub fn convert(items: &[ImportItem], ids: &[Option<String>]) -> Conversion {
    let mut conversion = Conversion::default();
    let mut candidates: Vec<Candidate> = Vec::new();
    for (index, item) in items.iter().enumerate() {
        if ids.get(index).is_none_or(Option::is_none) {
            continue;
        }
        for (field, text) in texts(item) {
            let refs = find_refs(text);
            if refs.is_empty() {
                continue;
            }
            let mut out = String::with_capacity(text.len());
            let mut last = 0;
            let mut converted_here = 0u32;
            for reference in &refs {
                out.push_str(&text[last..reference.start]);
                let kind = match reference.wanted {
                    'U' => Some(RefKind::Username),
                    'P' => Some(RefKind::Password),
                    _ => None,
                };
                let token = kind.and_then(|kind| {
                    let source = source_of(items, reference)?;
                    let id = ids.get(source)?.as_deref()?;
                    build_token(id, &kind)
                });
                match token {
                    Some(token) => {
                        out.push_str(&token);
                        conversion.converted += 1;
                        converted_here += 1;
                    }
                    None => {
                        out.push_str(&text[reference.start..reference.end]);
                        conversion.left_as_text += 1;
                    }
                }
                last = reference.end;
            }
            out.push_str(&text[last..]);
            if out != text {
                candidates.push((index, field, out, converted_here));
            }
        }
    }
    accept_without_cycles(items, ids, candidates, &mut conversion);
    conversion
}

/// A rewrite before the cycle check, with the number of references it converts.
type Candidate = (usize, ImportField, String, u32);

/// Takes the rewrites one by one and keeps a rewrite only when the entry's texts then lead back to
/// none of its own fields (spec 036, FR-046: a cycle is refused on save, and KeePass tolerates
/// them). A rewrite that would close a cycle stays text, and its references count as left.
fn accept_without_cycles(
    items: &[ImportItem],
    ids: &[Option<String>],
    candidates: Vec<Candidate>,
    conversion: &mut Conversion,
) {
    let by_id: HashMap<String, usize> = ids
        .iter()
        .enumerate()
        .filter_map(|(index, id)| id.as_ref().map(|id| (id.to_ascii_lowercase(), index)))
        .collect();
    let mut accepted: HashMap<(usize, ImportField), String> = HashMap::new();
    for (index, field, text, count) in candidates {
        accepted.insert((index, field), text.clone());
        let own = own_fields(items, index, &accepted);
        let mut lookup = |id: &str, kind: &RefKind| -> Result<Option<String>, Infallible> {
            Ok(by_id
                .get(id)
                .and_then(|&source| value_of(items, source, kind, &accepted)))
        };
        let item_id = ids[index]
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase();
        let cycle = match find_cycle(&item_id, &own, &mut lookup) {
            Ok(found) => found.is_some(),
            Err(never) => match never {},
        };
        if cycle {
            accepted.remove(&(index, field));
            conversion.converted -= count;
            conversion.left_as_text += count;
        } else {
            conversion.rewrites.push((index, field, text));
        }
    }
}

/// The text of a field of entry `index`, with the rewrites accepted so far.
fn text_of(
    items: &[ImportItem],
    index: usize,
    field: ImportField,
    accepted: &HashMap<(usize, ImportField), String>,
) -> Option<String> {
    if let Some(text) = accepted.get(&(index, field)) {
        return Some(text.clone());
    }
    let item = &items[index];
    match field {
        ImportField::Username => item.username.clone(),
        ImportField::Password => item.password.clone(),
        ImportField::Url => item.url.clone(),
        ImportField::Note => item.note.clone(),
        ImportField::KeyValue(position) => item.key_values.get(position)?.value.clone(),
    }
}

/// The value a placeholder reaches in entry `index` (the first custom field with the key).
fn value_of(
    items: &[ImportItem],
    index: usize,
    kind: &RefKind,
    accepted: &HashMap<(usize, ImportField), String>,
) -> Option<String> {
    let field = match kind {
        RefKind::Username => ImportField::Username,
        RefKind::Password => ImportField::Password,
        RefKind::Extra(key) => ImportField::KeyValue(
            items[index]
                .key_values
                .iter()
                .position(|kv| &kv.key == key)?,
        ),
    };
    Some(text_of(items, index, field, accepted).unwrap_or_default())
}

/// The reference fields of entry `index` as the cycle check names them.
fn own_fields(
    items: &[ImportItem],
    index: usize,
    accepted: &HashMap<(usize, ImportField), String>,
) -> Vec<(Field, String)> {
    let mut out = Vec::new();
    for (field, import_field) in [
        (Field::Username, ImportField::Username),
        (Field::Password, ImportField::Password),
        (Field::Url, ImportField::Url),
        (Field::Note, ImportField::Note),
    ] {
        if let Some(text) = text_of(items, index, import_field, accepted) {
            out.push((field, text));
        }
    }
    for (position, kv) in items[index].key_values.iter().enumerate() {
        if let Some(text) = text_of(items, index, ImportField::KeyValue(position), accepted) {
            out.push((Field::Extra(kv.key.clone()), text));
        }
    }
    out
}
