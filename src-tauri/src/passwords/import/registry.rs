//! The one import that may run at a time and the flag that cancels it (spec 034, US7): the window
//! starts a run, `passwords_import_cancel` sets the flag of the run that is going.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::error::{HaexError, Result};

#[derive(Default)]
pub struct ImportRegistry {
    current: Mutex<Option<Arc<AtomicBool>>>,
}

/// The right to run an import; dropping it frees the slot.
pub struct RunGuard {
    flag: Arc<AtomicBool>,
    registry: Arc<ImportRegistry>,
}

impl RunGuard {
    pub fn flag(&self) -> &AtomicBool {
        &self.flag
    }
}

impl Drop for RunGuard {
    fn drop(&mut self) {
        if let Ok(mut slot) = self.registry.current.lock() {
            *slot = None;
        }
    }
}

impl ImportRegistry {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Takes the slot; a second import while one runs is refused.
    pub fn begin(self: &Arc<Self>) -> Result<RunGuard> {
        let mut slot = self.current.lock().map_err(|_| HaexError::CrdtInit {
            reason: "import registry poisoned".to_string(),
        })?;
        if slot.is_some() {
            return Err(HaexError::InvalidInput {
                reason: "import_running".to_string(),
            });
        }
        let flag = Arc::new(AtomicBool::new(false));
        *slot = Some(Arc::clone(&flag));
        Ok(RunGuard {
            flag,
            registry: Arc::clone(self),
        })
    }

    /// Asks the running import to stop and undo what it wrote; nothing happens if none runs.
    pub fn cancel(&self) {
        if let Ok(slot) = self.current.lock() {
            if let Some(flag) = slot.as_ref() {
                flag.store(true, Ordering::SeqCst);
            }
        }
    }
}
