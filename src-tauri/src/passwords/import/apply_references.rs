//! The reference pass of an import (spec 036, FR-050, research R13): before the entries are
//! written, every entry that will be written gets its id, and the KeePass references in its texts
//! become haex-vault placeholders on those ids, so the first state of each entry already holds them.
//! The skipped duplicates are known before writing, so no reference points at an id that is never
//! written.

use uuid::Uuid;

use super::references::{convert, ImportField};
use super::ImportItem;

/// Gives every entry not skipped its id and converts the references; returns the numbers of
/// converted references and of those left as text.
pub fn prepare(items: &mut [ImportItem], skipped: &[bool]) -> (u32, u32) {
    let ids: Vec<Option<String>> = items
        .iter()
        .enumerate()
        .map(|(index, _)| {
            (!skipped.get(index).copied().unwrap_or(false)).then(|| Uuid::new_v4().to_string())
        })
        .collect();
    let conversion = convert(items, &ids);
    for (item, id) in items.iter_mut().zip(&ids) {
        item.assigned_id = id.clone();
    }
    for (index, field, text) in conversion.rewrites {
        let item = &mut items[index];
        match field {
            ImportField::Username => item.username = Some(text),
            ImportField::Password => item.password = Some(text),
            ImportField::Url => item.url = Some(text),
            ImportField::Note => item.note = Some(text),
            ImportField::KeyValue(position) => {
                if let Some(field) = item.key_values.get_mut(position) {
                    field.value = Some(text);
                }
            }
        }
    }
    (conversion.converted, conversion.left_as_text)
}
