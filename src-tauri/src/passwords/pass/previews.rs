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
