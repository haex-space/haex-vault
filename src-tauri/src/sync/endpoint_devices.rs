//! What a connection tells about the device on its other end (spec 024, FR-029,
//! FR-030, FR-033): a halted device and its reason, the device coming online
//! and going offline, and telling the interface about both. Part of
//! [`super`], so it works on the node's private state.

use std::sync::Arc;

use super::{Inner, Problem, RejectCode};
use crate::sync::handshake;
use crate::sync::peer_schema;
use crate::sync::problems;
use crate::sync::wire::{ErrorCode, SchemaVersion};

/// Records `device` as seen right now. Best-effort.
pub(super) async fn mark_seen(inner: &Arc<Inner>, device: [u8; 32]) {
    let replica = Arc::clone(&inner.replica);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    match tokio::task::spawn_blocking(move || crate::sync::seen::touch(&replica, &device, now))
        .await
    {
        Ok(Ok(_)) => {}
        Ok(Err(error)) => log::debug!("sync: recording a device's time failed: {error}"),
        Err(error) => log::debug!("sync: recording a device's time did not run: {error}"),
    }
}

/// Records the version state the device at `endpoint` named in a handshake (spec 059). The device
/// is the one the signed list gives that endpoint, never what the peer claims; an endpoint the
/// list does not name gets nothing. The list is told only when a value changed. Best-effort.
pub(super) async fn record_peer_schema(
    inner: &Arc<Inner>,
    endpoint: [u8; 32],
    schema: SchemaVersion,
) {
    let Some(device) = device_at(inner, endpoint).await else {
        return;
    };
    let replica = Arc::clone(&inner.replica);
    let changed =
        tokio::task::spawn_blocking(move || peer_schema::record(&replica, &device, schema))
            .await
            .map_err(|e| e.to_string())
            .and_then(|r| r.map_err(|e| e.to_string()));
    match changed {
        Ok(true) => notify_devices_changed(inner),
        Ok(false) => {}
        Err(error) => log::warn!("sync: recording a device's version state failed: {error}"),
    }
}

/// Turns a handshake failure into a halted device where it names one: the
/// device that proved its key, or, when the peer refused this device for
/// its version or as a duplicate, the device the list gives that endpoint.
pub(super) async fn note_failure(
    inner: &Arc<Inner>,
    error: &handshake::HandshakeError,
    remote: [u8; 32],
) {
    use handshake::HandshakeError::{Halted, Refused, RefusedByPeer};
    let (device, problem) = match error {
        Halted { problem, device } => (Some(*device), *problem),
        // The peer refused this device, or this device refused a peer of another protocol version
        // before it could read its state (`Challenge` of another version, spec 059 R2).
        RefusedByPeer(RejectCode::Incompatible) | Refused(RejectCode::Incompatible) => {
            (device_at(inner, remote).await, Problem::IncompatibleVersion)
        }
        RefusedByPeer(RejectCode::Duplicate) => {
            (device_at(inner, remote).await, Problem::Duplicate)
        }
        // This device may just have heard, from the list the peer pushed, that it was removed.
        RefusedByPeer(RejectCode::Removed) => {
            notify_devices_changed(inner);
            return;
        }
        _ => return,
    };
    if let Some(device) = device {
        flag(inner, device, problem).await;
    }
}

async fn device_at(inner: &Arc<Inner>, endpoint: [u8; 32]) -> Option<[u8; 32]> {
    let replica = Arc::clone(&inner.replica);
    let vault = inner.vault;
    tokio::task::spawn_blocking(move || problems::device_at_endpoint(&replica, vault, &endpoint))
        .await
        .ok()?
        .ok()?
}

pub(super) async fn flag(inner: &Arc<Inner>, device: [u8; 32], problem: Problem) {
    if problem == Problem::Duplicate {
        inner.duplicates.note(device);
        let live = inner
            .peers
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&device);
        if let Some(live) = live {
            live.close(ErrorCode::Rejected.as_u32().into(), b"duplicate");
        }
    }
    let replica = Arc::clone(&inner.replica);
    let changed = tokio::task::spawn_blocking(move || problems::set(&replica, &device, problem))
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r.map_err(|e| e.to_string()));
    match changed {
        Ok(true) => notify_devices_changed(inner),
        Ok(false) => {}
        Err(error) => log::warn!("sync: recording a device problem failed: {error}"),
    }
}

pub(super) async fn clear_problem(inner: &Arc<Inner>, device: [u8; 32]) {
    let replica = Arc::clone(&inner.replica);
    let cleared = tokio::task::spawn_blocking(move || problems::clear(&replica, &device))
        .await
        .map_err(|e| e.to_string())
        .and_then(|r| r.map_err(|e| e.to_string()));
    match cleared {
        Ok(true) => notify_devices_changed(inner),
        Ok(false) => {}
        Err(error) => log::warn!("sync: clearing a device problem failed: {error}"),
    }
}

pub(super) fn notify_devices_changed(inner: &Inner) {
    let hook = inner
        .devices_changed
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone();
    if let Some(hook) = hook {
        hook();
    }
}
