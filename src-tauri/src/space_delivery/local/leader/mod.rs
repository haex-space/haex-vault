//! Leader-side delivery: connection handler, request dispatch, state management.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;

use tokio::sync::RwLock;

use tauri::AppHandle;

use super::invite_tokens::LocalInviteToken;
use super::protocol::Notification;
use super::types::ConnectedPeer;
use crate::crdt::hlc::HlcService;
use crate::database::DbConnection;

mod auth;
mod claim;
mod dispatch;
mod notify;
mod util;

#[cfg(test)]
mod tests;

pub use claim::handle_claim_invite;
pub(super) use dispatch::{handle_delivery_request, send_response};

// ============================================================================
// State
// ============================================================================

/// State held by the leader for active delivery sessions.
pub struct LeaderState {
    /// Database connection
    pub db: DbConnection,
    /// HLC service for CRDT-synced writes
    pub hlc: Arc<Mutex<HlcService>>,
    /// Tauri AppHandle for emitting events to the frontend
    pub app_handle: AppHandle,
    /// Space ID this leader serves
    pub space_id: String,
    /// Currently connected peers (endpoint_id → peer info) — IN-MEMORY ONLY, never persisted
    pub connected_peers: Arc<RwLock<HashMap<String, ConnectedPeer>>>,
    /// Notification senders for connected peers (endpoint_id → sender)
    pub notification_senders: Arc<RwLock<HashMap<String, tokio::sync::mpsc::Sender<Notification>>>>,
    /// In-memory invite tokens (loaded from DB on start, synced back on changes)
    pub invite_tokens: Arc<RwLock<Vec<LocalInviteToken>>>,
    /// Sliding-window reject-rate counter for L4 DoS-defence. Per-DID
    /// counters; shared by every `authorize_request` invocation on this
    /// leader. Lifetime = leader-lifetime, lost on restart by design.
    pub reject_tracker: Arc<super::dos_defence::tracker::RejectRateTracker>,
    /// Cached DoS-defence config loaded once on leader start. Re-loaded
    /// only on full leader restart — Phase 1 favours simplicity over
    /// hot-reload.
    pub dos_config: Arc<super::dos_defence::config::DosDefenceConfig>,
    /// Per-DID one-shot guard for single-source-flood notifications.
    /// Without this every reject above the warn threshold would re-emit
    /// the banner, drowning the user in identical rows during a flood.
    pub flood_notifier: Arc<super::dos_defence::notifier::SingleSourceNotifier>,
    /// Snapshot of the global critical-notification sink at leader-start
    /// time. `None` when the vault was opened without a sink (tests /
    /// pre-open). `CriticalNotificationSink` is `Clone`-cheap (internal
    /// `Arc`).
    pub critical_sink: Option<crate::critical::sink::CriticalNotificationSink>,
}
