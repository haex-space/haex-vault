//! The passes read from files and waiting for the user's choice (spec 092, FR-031, FR-035): held in
//! memory only, so a file is not written anywhere, and dropped when the dialog saves or cancels.
//! At most a few are kept; a new one pushes the oldest out.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use uuid::Uuid;

use super::pkpass::ParsedPass;

/// How many files may wait at once: a dialog handles several files one by one.
const CAPACITY: usize = 16;

#[derive(Default)]
pub struct PassImportPreviews {
    slots: Mutex<VecDeque<(String, ParsedPass)>>,
}

impl PassImportPreviews {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Keeps a pass and returns the id the dialog uses for it.
    pub fn put(&self, pass: ParsedPass) -> String {
        let id = Uuid::new_v4().to_string();
        if let Ok(mut slots) = self.slots.lock() {
            if slots.len() >= CAPACITY {
                slots.pop_front();
            }
            slots.push_back((id.clone(), pass));
        }
        id
    }

    /// The pass behind an id, kept.
    pub fn get(&self, id: &str) -> Option<ParsedPass> {
        let slots = self.slots.lock().ok()?;
        slots
            .iter()
            .find(|(key, _)| key == id)
            .map(|(_, p)| p.clone())
    }

    /// Takes the pass behind an id out, so that one save has it and a second one at the same time
    /// finds nothing. A save that fails gives it back with [`Self::release`].
    pub fn take(&self, id: &str) -> Option<ParsedPass> {
        let mut slots = self.slots.lock().ok()?;
        let at = slots.iter().position(|(key, _)| key == id)?;
        slots.remove(at).map(|(_, pass)| pass)
    }

    /// Gives a pass taken with [`Self::take`] back under its id.
    pub fn release(&self, id: &str, pass: ParsedPass) {
        if let Ok(mut slots) = self.slots.lock() {
            if slots.len() >= CAPACITY {
                slots.pop_front();
            }
            slots.push_back((id.to_owned(), pass));
        }
    }

    /// Drops the pass behind an id.
    pub fn remove(&self, id: &str) {
        if let Ok(mut slots) = self.slots.lock() {
            slots.retain(|(key, _)| key != id);
        }
    }

    pub fn len(&self) -> usize {
        self.slots.lock().map_or(0, |slots| slots.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::passwords::pass::model::PassKind;

    fn pass(title: &str) -> ParsedPass {
        ParsedPass {
            pass_kind: PassKind::Other,
            ticket_like: false,
            title: title.to_owned(),
            provider: None,
            start: None,
            expires_at: None,
            codes: Vec::new(),
            key_values: Vec::new(),
            icon: None,
            source_id: None,
            not_kept: Vec::new(),
            signed: None,
        }
    }

    #[test]
    fn a_preview_is_kept_until_it_is_removed() {
        let previews = PassImportPreviews::new();
        let id = previews.put(pass("one"));
        assert_eq!(previews.get(&id).map(|p| p.title), Some("one".to_owned()));
        previews.remove(&id);
        assert!(previews.get(&id).is_none());
        assert!(previews.is_empty());
    }

    #[test]
    fn a_preview_taken_for_a_save_is_not_there_for_a_second_one_until_it_is_released() {
        let previews = PassImportPreviews::new();
        let id = previews.put(pass("one"));
        let taken = previews.take(&id).expect("taken");
        assert!(previews.take(&id).is_none());
        previews.release(&id, taken);
        assert!(previews.take(&id).is_some());
    }

    #[test]
    fn a_new_preview_past_the_capacity_pushes_the_oldest_out() {
        let previews = PassImportPreviews::new();
        let first = previews.put(pass("first"));
        let rest: Vec<String> = (0..CAPACITY)
            .map(|i| previews.put(pass(&format!("p{i}"))))
            .collect();
        assert_eq!(previews.len(), CAPACITY);
        assert!(previews.get(&first).is_none());
        assert!(previews.get(&rest[CAPACITY - 1]).is_some());
    }
}
