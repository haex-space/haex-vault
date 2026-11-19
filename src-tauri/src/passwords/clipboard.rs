//! Copying to the clipboard with a timed clearing, in Rust (spec 034, FR-006, research R9). The
//! value goes from the vault to the clipboard without passing the webview; after the chosen time
//! the clipboard is cleared, but only if it still holds the value that was copied.
//!
//! The clipboard is reached through [`ClipboardPort`], so the timing can be tested with a fake
//! port and paused time.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use tauri::AppHandle;
use tauri_plugin_clipboard_manager::ClipboardExt;
use tokio::task::JoinHandle;
use zeroize::Zeroizing;

use crate::error::{HaexError, Result};

/// The clipboard as the clearer needs it.
pub trait ClipboardPort: Send + Sync + 'static {
    fn write(&self, text: &str) -> Result<()>;
    /// The text on the clipboard; `None` when it holds none.
    fn read(&self) -> Result<Option<String>>;
    fn clear(&self) -> Result<()>;
}

/// An error that names the step, never the content.
fn failed(step: &str) -> HaexError {
    HaexError::Io {
        reason: format!("clipboard {step} failed"),
    }
}

impl ClipboardPort for AppHandle {
    fn write(&self, text: &str) -> Result<()> {
        self.clipboard()
            .write_text(text.to_string())
            .map_err(|_| failed("write"))
    }

    fn read(&self) -> Result<Option<String>> {
        // An empty clipboard or one with an image reads as an error of the plugin; both mean
        // "not our value".
        Ok(self.clipboard().read_text().ok())
    }

    fn clear(&self) -> Result<()> {
        self.clipboard().clear().map_err(|_| failed("clear"))
    }
}

/// What a pending clearing needs: the timer, the value to compare, the clipboard.
struct Pending {
    task: JoinHandle<()>,
    value: Arc<Zeroizing<String>>,
    port: Arc<dyn ClipboardPort>,
}

/// Holds the one pending clearing: a new copy cancels the one before it.
#[derive(Default)]
pub struct ClipboardClearer {
    pending: Mutex<Option<Pending>>,
}

impl ClipboardClearer {
    pub fn new() -> Self {
        Self::default()
    }

    /// Writes `value` and, if `clear_after` is set, clears the clipboard after that time when it
    /// still holds `value`. A second copy cancels the first timer. Must run inside a tokio
    /// runtime when a delay is set.
    // ponytail: a crash between the copy and the clearing leaves the value on the clipboard
    // (ceiling: a copied secret survives a crash; upgrade path: none on desktop, clipboard
    // managers keep history anyway).
    pub fn copy(
        &self,
        port: Arc<dyn ClipboardPort>,
        value: &str,
        clear_after: Option<Duration>,
    ) -> Result<()> {
        self.cancel();
        port.write(value)?;
        let Some(delay) = clear_after else {
            return Ok(());
        };
        let copied = Arc::new(Zeroizing::new(value.to_string()));
        let task = {
            let (port, copied) = (Arc::clone(&port), Arc::clone(&copied));
            tokio::spawn(async move {
                tokio::time::sleep(delay).await;
                clear_if_unchanged(port.as_ref(), &copied);
            })
        };
        if let Ok(mut slot) = self.pending.lock() {
            *slot = Some(Pending {
                task,
                value: copied,
                port,
            });
        }
        Ok(())
    }

    /// Clears now what a pending timer would clear later (the vault is closing): cancels the timer
    /// and clears the clipboard if it still holds the copied value.
    pub fn clear_now(&self) {
        let pending = self.pending.lock().ok().and_then(|mut slot| slot.take());
        if let Some(pending) = pending {
            pending.task.abort();
            clear_if_unchanged(pending.port.as_ref(), &pending.value);
        }
    }

    fn cancel(&self) {
        if let Some(pending) = self.pending.lock().ok().and_then(|mut slot| slot.take()) {
            pending.task.abort();
        }
    }
}

/// Clears the clipboard only if it still holds the copied value; failures are logged without the
/// value.
fn clear_if_unchanged(port: &dyn ClipboardPort, copied: &Zeroizing<String>) {
    match port.read() {
        Ok(Some(current)) if current == copied.as_str() => {
            if let Err(error) = port.clear() {
                log::warn!("could not clear the clipboard: {error}");
            }
        }
        Ok(_) => {}
        Err(error) => log::warn!("could not read the clipboard: {error}"),
    }
}
