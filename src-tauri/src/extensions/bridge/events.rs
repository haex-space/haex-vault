//! Messages from haex-vault to the frames of one extension (contracts/bridge.md §Meldungen): Rust
//! sends `extension-frame-event {frame, type, data, timestamp}` to haex-vault's window for every open
//! frame of that extension; the window hands it to the frame's port. Frames of other extensions
//! get nothing.

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Value};
use uuid::Uuid;

use super::dispatch::Emit;
use crate::extensions::host::ExtensionHost;

pub const FRAME_EVENT: &str = "extension-frame-event";

/// Sends `event_type` to every open frame of `extension_id`.
pub fn emit_to_frames(
    emitter: &dyn Emit,
    host: &ExtensionHost,
    extension_id: Uuid,
    event_type: &str,
    data: &Value,
) {
    emit_to_frames_counted(emitter, host, extension_id, event_type, data, |_| {});
}

/// [`emit_to_frames`], telling `before` each frame right before its event goes out: whatever the
/// frame answers to the event comes after `before` saw it.
pub fn emit_to_frames_counted(
    emitter: &dyn Emit,
    host: &ExtensionHost,
    extension_id: Uuid,
    event_type: &str,
    data: &Value,
    mut before: impl FnMut(&str),
) {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64);
    for session in host.frames.of_extension(extension_id) {
        before(&session.frame);
        emitter.emit(
            FRAME_EVENT,
            json!({
                "frame": session.frame,
                "type": event_type,
                "data": data,
                "timestamp": timestamp,
            }),
        );
    }
}
